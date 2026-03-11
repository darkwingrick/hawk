use anyhow::{Context as _, Result};
use client::Client;
use db::kvp::KEY_VALUE_STORE;
use futures_lite::StreamExt;
use gpui::{
    App, AppContext as _, AsyncApp, BackgroundExecutor, Context, Entity, Global, Task, Window,
    actions,
};
use http_client::github::{get_release_by_tag_name, latest_github_release};
use http_client::{HttpClient, HttpClientWithUrl};
use paths::remote_servers_dir;
use release_channel::{AppCommitSha, ReleaseChannel};
use semver::Version;
use serde::{Deserialize, Serialize};
use settings::{RegisterSetting, Settings, SettingsStore};
use sha2::{Digest, Sha256};
use smol::fs::File;
use smol::{fs, io::AsyncReadExt};
use std::mem;
use std::{
    env::{
        self,
        consts::{ARCH, OS},
    },
    ffi::OsStr,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use util::command::new_command;
use workspace::Workspace;

const SHOULD_SHOW_UPDATE_NOTIFICATION_KEY: &str = "auto-updater-should-show-updated-notification";
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);
const REMOTE_SERVER_CACHE_LIMIT: usize = 5;
const DEFAULT_RELEASE_REPOSITORY: &str = "darkwingrick/hawk";
const RELEASE_REPOSITORY_ENV_VAR: &str = "HAWK_RELEASE_REPOSITORY";
const DEFAULT_TAG_PREFIX: &str = "v";
const RELEASE_TAG_PREFIX_ENV_VAR: &str = "HAWK_RELEASE_TAG_PREFIX";
const CHECKSUM_ASSET_NAME: &str = "SHA256SUMS";

actions!(
    auto_update,
    [
        /// Checks for available updates.
        Check,
        /// Dismisses the update error message.
        DismissMessage,
        /// Opens the release notes for the current version in a browser.
        ViewReleaseNotes,
    ]
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionCheckType {
    Sha(AppCommitSha),
    Semantic(Version),
}

#[derive(Serialize, Debug)]
pub struct AssetQuery<'a> {
    asset: &'a str,
    os: &'a str,
    arch: &'a str,
    metrics_id: Option<&'a str>,
    system_id: Option<&'a str>,
    is_staff: Option<bool>,
}

#[derive(Clone, Debug)]
pub enum AutoUpdateStatus {
    Idle,
    Checking,
    Downloading { version: VersionCheckType },
    Installing { version: VersionCheckType },
    Updated { version: VersionCheckType },
    Errored { error: Arc<anyhow::Error> },
}

impl PartialEq for AutoUpdateStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AutoUpdateStatus::Idle, AutoUpdateStatus::Idle) => true,
            (AutoUpdateStatus::Checking, AutoUpdateStatus::Checking) => true,
            (
                AutoUpdateStatus::Downloading { version: v1 },
                AutoUpdateStatus::Downloading { version: v2 },
            ) => v1 == v2,
            (
                AutoUpdateStatus::Installing { version: v1 },
                AutoUpdateStatus::Installing { version: v2 },
            ) => v1 == v2,
            (
                AutoUpdateStatus::Updated { version: v1 },
                AutoUpdateStatus::Updated { version: v2 },
            ) => v1 == v2,
            (AutoUpdateStatus::Errored { error: e1 }, AutoUpdateStatus::Errored { error: e2 }) => {
                e1.to_string() == e2.to_string()
            }
            _ => false,
        }
    }
}

impl AutoUpdateStatus {
    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated { .. })
    }
}

pub struct AutoUpdater {
    status: AutoUpdateStatus,
    current_version: Version,
    client: Arc<Client>,
    pending_poll: Option<Task<Option<()>>>,
    quit_subscription: Option<gpui::Subscription>,
    update_check_type: UpdateCheckType,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ReleaseAsset {
    pub version: String,
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
}

struct MacOsUnmounter<'a> {
    mount_path: PathBuf,
    background_executor: &'a BackgroundExecutor,
}

impl Drop for MacOsUnmounter<'_> {
    fn drop(&mut self) {
        let mount_path = mem::take(&mut self.mount_path);
        self.background_executor
            .spawn(async move {
                let unmount_output = new_command("hdiutil")
                    .args(["detach", "-force"])
                    .arg(&mount_path)
                    .output()
                    .await;
                match unmount_output {
                    Ok(output) if output.status.success() => {
                        log::info!("Successfully unmounted the disk image");
                    }
                    Ok(output) => {
                        log::error!(
                            "Failed to unmount disk image: {:?}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(error) => {
                        log::error!("Error while trying to unmount disk image: {:?}", error);
                    }
                }
            })
            .detach();
    }
}

#[derive(Clone, Copy, Debug, RegisterSetting)]
struct AutoUpdateSetting(bool);

/// Whether or not to automatically check for updates.
///
/// Default: true
impl Settings for AutoUpdateSetting {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self(content.auto_update.unwrap())
    }
}

#[derive(Default)]
struct GlobalAutoUpdate(Option<Entity<AutoUpdater>>);

impl Global for GlobalAutoUpdate {}

pub fn init(client: Arc<Client>, cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|_, action, window, cx| check(action, window, cx));

        workspace.register_action(|_, action, _, cx| {
            view_release_notes(action, cx);
        });
    })
    .detach();

    let version = release_channel::AppVersion::global(cx);
    let auto_updater = cx.new(|cx| {
        let updater = AutoUpdater::new(version, client, cx);

        let poll_for_updates = ReleaseChannel::try_global(cx)
            .map(|channel| channel.poll_for_updates())
            .unwrap_or(false);

        if option_env!("HAWK_UPDATE_EXPLANATION").is_none()
            && env::var("HAWK_UPDATE_EXPLANATION").is_err()
            && poll_for_updates
        {
            let mut update_subscription = AutoUpdateSetting::get_global(cx)
                .0
                .then(|| updater.start_polling(cx));

            cx.observe_global::<SettingsStore>(move |updater: &mut AutoUpdater, cx| {
                if AutoUpdateSetting::get_global(cx).0 {
                    if update_subscription.is_none() {
                        update_subscription = Some(updater.start_polling(cx))
                    }
                } else {
                    update_subscription.take();
                }
            })
            .detach();
        }

        updater
    });
    cx.set_global(GlobalAutoUpdate(Some(auto_updater)));
}

pub fn check(_: &Check, window: &mut Window, cx: &mut App) {
    if let Some(message) = option_env!("HAWK_UPDATE_EXPLANATION") {
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "Hawk was installed via a package manager.",
            Some(message),
            &["Ok"],
            cx,
        ));
        return;
    }

    if let Ok(message) = env::var("HAWK_UPDATE_EXPLANATION") {
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "Hawk was installed via a package manager.",
            Some(&message),
            &["Ok"],
            cx,
        ));
        return;
    }

    if !ReleaseChannel::try_global(cx)
        .map(|channel| channel.poll_for_updates())
        .unwrap_or(false)
    {
        return;
    }

    if let Some(updater) = AutoUpdater::get(cx) {
        updater.update(cx, |updater, cx| updater.poll(UpdateCheckType::Manual, cx));
    } else {
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "Could not check for updates",
            Some("Auto-updates disabled for non-bundled app."),
            &["Ok"],
            cx,
        ));
    }
}

fn release_repository() -> String {
    match env::var(RELEASE_REPOSITORY_ENV_VAR) {
        Ok(repository) if release_repository_is_valid(&repository) => repository,
        Ok(invalid_repository) => {
            log::warn!(
                "{RELEASE_REPOSITORY_ENV_VAR} is invalid ({invalid_repository:?}), using default repository"
            );
            DEFAULT_RELEASE_REPOSITORY.to_string()
        }
        Err(_) => DEFAULT_RELEASE_REPOSITORY.to_string(),
    }
}

fn release_repository_is_valid(repository: &str) -> bool {
    let mut segments = repository.split('/');
    matches!(
        (segments.next(), segments.next(), segments.next()),
        (Some(owner), Some(repo), None) if !owner.is_empty() && !repo.is_empty()
    )
}

fn release_tag_prefix() -> String {
    match env::var(RELEASE_TAG_PREFIX_ENV_VAR) {
        Ok(prefix) if !prefix.is_empty() => prefix,
        Ok(_) => DEFAULT_TAG_PREFIX.to_string(),
        Err(_) => DEFAULT_TAG_PREFIX.to_string(),
    }
}

fn release_tag_for_version(version: &Version) -> String {
    format!("{}{}", release_tag_prefix(), version)
}

pub fn release_notes_url(cx: &mut App) -> Option<String> {
    let release_channel = ReleaseChannel::try_global(cx)?;
    let repository = release_repository();
    let url = match release_channel {
        ReleaseChannel::Stable | ReleaseChannel::Preview => {
            let auto_updater = AutoUpdater::get(cx)?;
            let auto_updater = auto_updater.read(cx);
            let mut current_version = auto_updater.current_version.clone();
            current_version.pre = semver::Prerelease::EMPTY;
            current_version.build = semver::BuildMetadata::EMPTY;
            let tag = release_tag_for_version(&current_version);
            Some(format!(
                "https://github.com/{repository}/releases/tag/{tag}"
            ))
        }
        ReleaseChannel::Nightly => {
            Some(format!("https://github.com/{repository}/releases/nightly/"))
        }
        ReleaseChannel::Dev => Some(format!("https://github.com/{repository}/releases/")),
    };
    url
}

pub fn view_release_notes(_: &ViewReleaseNotes, cx: &mut App) -> Option<()> {
    let url = release_notes_url(cx)?;
    cx.open_url(&url);
    None
}

#[cfg(not(target_os = "windows"))]
struct InstallerDir(tempfile::TempDir);

#[cfg(not(target_os = "windows"))]
impl InstallerDir {
    async fn new() -> Result<Self> {
        Ok(Self(
            tempfile::Builder::new()
                .prefix("zed-auto-update")
                .tempdir()?,
        ))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(target_os = "windows")]
struct InstallerDir(PathBuf);

#[cfg(target_os = "windows")]
impl InstallerDir {
    async fn new() -> Result<Self> {
        let installer_dir = std::env::current_exe()?
            .parent()
            .context("No parent dir for Hawk.exe")?
            .join("updates");
        if smol::fs::metadata(&installer_dir).await.is_ok() {
            smol::fs::remove_dir_all(&installer_dir).await?;
        }
        smol::fs::create_dir(&installer_dir).await?;
        Ok(Self(installer_dir))
    }

    fn path(&self) -> &Path {
        self.0.as_path()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UpdateCheckType {
    Automatic,
    Manual,
}

impl UpdateCheckType {
    pub fn is_manual(self) -> bool {
        self == Self::Manual
    }
}

impl AutoUpdater {
    pub fn get(cx: &mut App) -> Option<Entity<Self>> {
        cx.default_global::<GlobalAutoUpdate>().0.clone()
    }

    fn new(current_version: Version, client: Arc<Client>, cx: &mut Context<Self>) -> Self {
        // On windows, executable files cannot be overwritten while they are
        // running, so we must wait to overwrite the application until quitting
        // or restarting. When quitting the app, we spawn the auto update helper
        // to finish the auto update process after Hawk exits. When restarting
        // the app after an update, we use `set_restart_path` to run the auto
        // update helper instead of the app, so that it can overwrite the app
        // and then spawn the new binary.
        #[cfg(target_os = "windows")]
        let quit_subscription = Some(cx.on_app_quit(|_, _| finalize_auto_update_on_quit()));
        #[cfg(not(target_os = "windows"))]
        let quit_subscription = None;

        cx.on_app_restart(|this, _| {
            this.quit_subscription.take();
        })
        .detach();

        Self {
            status: AutoUpdateStatus::Idle,
            current_version,
            client,
            pending_poll: None,
            quit_subscription,
            update_check_type: UpdateCheckType::Automatic,
        }
    }

    pub fn start_polling(&self, cx: &mut Context<Self>) -> Task<Result<()>> {
        cx.spawn(async move |this, cx| {
            if cfg!(target_os = "windows") {
                use util::ResultExt;

                cleanup_windows()
                    .await
                    .context("failed to cleanup old directories")
                    .log_err();
            }

            loop {
                this.update(cx, |this, cx| this.poll(UpdateCheckType::Automatic, cx))?;
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        })
    }

    pub fn update_check_type(&self) -> UpdateCheckType {
        self.update_check_type
    }

    pub fn poll(&mut self, check_type: UpdateCheckType, cx: &mut Context<Self>) {
        if self.pending_poll.is_some() {
            return;
        }
        self.update_check_type = check_type;

        cx.notify();

        self.pending_poll = Some(cx.spawn(async move |this, cx| {
            let result = Self::update(this.upgrade()?, cx).await;
            this.update(cx, |this, cx| {
                this.pending_poll = None;
                if let Err(error) = result {
                    this.status = match check_type {
                        // Be quiet if the check was automated (e.g. when offline)
                        UpdateCheckType::Automatic => {
                            log::info!("auto-update check failed: error:{:?}", error);
                            AutoUpdateStatus::Idle
                        }
                        UpdateCheckType::Manual => {
                            log::error!("auto-update failed: error:{:?}", error);
                            AutoUpdateStatus::Errored {
                                error: Arc::new(error),
                            }
                        }
                    };

                    cx.notify();
                }
            })
            .ok()
        }));
    }

    pub fn current_version(&self) -> Version {
        self.current_version.clone()
    }

    pub fn status(&self) -> AutoUpdateStatus {
        self.status.clone()
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if let AutoUpdateStatus::Idle = self.status {
            return false;
        }
        self.status = AutoUpdateStatus::Idle;
        cx.notify();
        true
    }

    // If you are packaging Zed and need to override the place it downloads SSH remotes from,
    // you can override this function. You should also update get_remote_server_release_url to return
    // Ok(None).
    pub async fn download_remote_server_release(
        release_channel: ReleaseChannel,
        version: Option<Version>,
        os: &str,
        arch: &str,
        set_status: impl Fn(&str, &mut AsyncApp) + Send + 'static,
        cx: &mut AsyncApp,
    ) -> Result<PathBuf> {
        let this = cx.update(|cx| {
            cx.default_global::<GlobalAutoUpdate>()
                .0
                .clone()
                .context("auto-update not initialized")
        })?;

        set_status("Fetching remote server release", cx);
        let release = Self::get_release_asset(
            &this,
            release_channel,
            version,
            "zed-remote-server",
            os,
            arch,
            cx,
        )
        .await?;

        let servers_dir = paths::remote_servers_dir();
        let channel_dir = servers_dir.join(release_channel.dev_name());
        let platform_dir = channel_dir.join(format!("{}-{}", os, arch));
        let version_path = platform_dir.join(format!("{}.gz", release.version));
        smol::fs::create_dir_all(&platform_dir).await.ok();

        let client = this.read_with(cx, |this, _| this.client.http_client());

        if smol::fs::metadata(&version_path).await.is_err() {
            log::info!(
                "downloading zed-remote-server {os} {arch} version {}",
                release.version
            );
            set_status("Downloading remote server", cx);
            download_remote_server_binary(&version_path, release, client).await?;
        }

        if let Err(error) =
            cleanup_remote_server_cache(&platform_dir, &version_path, REMOTE_SERVER_CACHE_LIMIT)
                .await
        {
            log::warn!(
                "Failed to clean up remote server cache in {:?}: {error:#}",
                platform_dir
            );
        }

        Ok(version_path)
    }

    pub async fn get_remote_server_release_url(
        channel: ReleaseChannel,
        version: Option<Version>,
        os: &str,
        arch: &str,
        cx: &mut AsyncApp,
    ) -> Result<Option<String>> {
        let this = cx.update(|cx| {
            cx.default_global::<GlobalAutoUpdate>()
                .0
                .clone()
                .context("auto-update not initialized")
        })?;

        let release =
            Self::get_release_asset(&this, channel, version, "zed-remote-server", os, arch, cx)
                .await?;

        Ok(Some(release.url))
    }

    async fn get_release_asset(
        this: &Entity<Self>,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        asset_name_prefix: &str,
        os: &str,
        arch: &str,
        cx: &mut AsyncApp,
    ) -> Result<ReleaseAsset> {
        let client = this.read_with(cx, |this, _| this.client.clone());
        let repository = release_repository();
        let release_tag_prefix = release_tag_prefix();

        let http_client = client.http_client();
        let http_client_dyn: Arc<dyn HttpClient> = http_client.clone();
        let include_prereleases = matches!(
            release_channel,
            ReleaseChannel::Nightly | ReleaseChannel::Preview
        );

        let github_release = if let Some(mut requested_version) = version {
            requested_version.pre = semver::Prerelease::EMPTY;
            requested_version.build = semver::BuildMetadata::EMPTY;
            let release_tag = format!("{release_tag_prefix}{requested_version}");
            get_release_by_tag_name(&repository, &release_tag, http_client_dyn.clone()).await?
        } else if include_prereleases {
            match latest_github_release(&repository, true, true, http_client_dyn.clone()).await {
                Ok(release) => release,
                Err(prerelease_error) => latest_github_release(
                    &repository,
                    true,
                    false,
                    http_client_dyn.clone(),
                )
                .await
                .with_context(|| {
                    format!("failed to fetch prerelease ({prerelease_error:#}) and stable release")
                })?,
            }
        } else {
            latest_github_release(&repository, true, false, http_client_dyn.clone()).await?
        };

        let normalized_version = github_release
            .tag_name
            .strip_prefix(&release_tag_prefix)
            .unwrap_or(&github_release.tag_name)
            .to_string();

        let target_patterns = Self::get_asset_pattern(os, arch);
        let expected_extension = Self::expected_asset_extension(asset_name_prefix, os)?;
        let release_asset = github_release
            .assets
            .iter()
            .find(|a| {
                Self::asset_matches(
                    &a.name,
                    asset_name_prefix,
                    &target_patterns,
                    expected_extension,
                )
            })
            .with_context(|| {
                format!(
                    "no matching release asset found for {os} {arch}, prefix {asset_name_prefix}, extension {expected_extension}"
                )
            })?;

        let checksum_from_asset = release_asset
            .digest
            .as_deref()
            .and_then(normalize_sha256_digest);
        let checksum_from_sums_file = Self::checksum_from_release_sums(
            &github_release.assets,
            &release_asset.name,
            http_client,
        )
        .await?;
        let checksum = checksum_from_asset.or(checksum_from_sums_file);

        Ok(ReleaseAsset {
            version: normalized_version,
            name: release_asset.name.clone(),
            url: release_asset.browser_download_url.clone(),
            sha256: checksum,
        })
    }

    /// Get the asset name pattern for the current OS and architecture
    /// Returns patterns to match against GitHub asset names
    fn get_asset_pattern(os: &str, arch: &str) -> Vec<String> {
        let mut patterns = Vec::new();

        let os_patterns: Vec<&str> = match os {
            "macos" => vec!["darwin", "macos", "osx"],
            "linux" => vec!["linux"],
            "windows" => vec!["windows", "win"],
            _ => vec![os],
        };

        let arch_patterns: Vec<&str> = match arch {
            "x86_64" | "x86" => vec!["x86_64", "x86-64", "amd64"],
            "aarch64" | "arm64" => vec!["aarch64", "arm64", "armv8"],
            _ => vec![arch],
        };

        // Generate all combinations of OS and arch patterns
        for os_pat in &os_patterns {
            for arch_pat in &arch_patterns {
                patterns.push(format!("{}-{}", os_pat, arch_pat));
            }
        }

        patterns
    }

    fn expected_asset_extension(asset_name_prefix: &str, os: &str) -> Result<&'static str> {
        let is_remote_server_asset = asset_name_prefix.contains("remote-server");
        if is_remote_server_asset {
            return match os {
                "macos" | "linux" | "windows" => Ok(""),
                unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
            };
        }

        match os {
            "macos" => Ok(".dmg"),
            "linux" => Ok(".tar.gz"),
            "windows" => Ok(".exe"),
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        }
    }

    fn asset_matches(
        asset_name: &str,
        asset_name_prefix: &str,
        target_patterns: &[String],
        expected_extension: &str,
    ) -> bool {
        let normalized_name = asset_name.to_ascii_lowercase();
        let normalized_prefix = asset_name_prefix.to_ascii_lowercase();
        let normalized_extension = expected_extension.to_ascii_lowercase();

        let matches_prefix = normalized_name.contains(&normalized_prefix);
        let matches_platform = target_patterns
            .iter()
            .any(|pattern| normalized_name.contains(&pattern.to_ascii_lowercase()));
        let matches_extension =
            normalized_extension.is_empty() || normalized_name.ends_with(&normalized_extension);

        matches_prefix && matches_platform && matches_extension
    }

    async fn checksum_from_release_sums(
        release_assets: &[http_client::github::GithubReleaseAsset],
        target_asset_name: &str,
        http_client: Arc<HttpClientWithUrl>,
    ) -> Result<Option<String>> {
        let checksum_asset = release_assets.iter().find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name == CHECKSUM_ASSET_NAME.to_ascii_lowercase()
                || name == format!("{}.txt", CHECKSUM_ASSET_NAME.to_ascii_lowercase())
        });

        let Some(checksum_asset) = checksum_asset else {
            return Ok(None);
        };

        let mut response = http_client
            .get(
                &checksum_asset.browser_download_url,
                Default::default(),
                true,
            )
            .await
            .with_context(|| {
                format!(
                    "failed to download checksum file {}",
                    checksum_asset.browser_download_url
                )
            })?;
        anyhow::ensure!(
            response.status().is_success(),
            "failed to fetch checksum file {}: {}",
            checksum_asset.browser_download_url,
            response.status()
        );

        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;
        let checksum_contents = String::from_utf8(body)
            .with_context(|| format!("invalid utf-8 in checksum file {}", checksum_asset.name))?;

        Ok(parse_sha256sums(&checksum_contents, target_asset_name))
    }

    async fn update(this: Entity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let (client, installed_version, previous_status, release_channel) =
            this.read_with(cx, |this, cx| {
                (
                    this.client.http_client(),
                    this.current_version.clone(),
                    this.status.clone(),
                    ReleaseChannel::try_global(cx).unwrap_or(ReleaseChannel::Stable),
                )
            });

        Self::check_dependencies()?;

        this.update(cx, |this, cx| {
            this.status = AutoUpdateStatus::Checking;
            log::info!("Auto Update: checking for updates");
            cx.notify();
        });

        let fetched_release_data =
            Self::get_release_asset(&this, release_channel, None, "hawk", OS, ARCH, cx).await?;
        let fetched_version = fetched_release_data.clone().version;
        let app_commit_sha = Ok(cx.update(|cx| AppCommitSha::try_global(cx).map(|sha| sha.full())));
        let newer_version = Self::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_version,
            previous_status.clone(),
        )?;

        let Some(newer_version) = newer_version else {
            this.update(cx, |this, cx| {
                let status = match previous_status {
                    AutoUpdateStatus::Updated { .. } => previous_status,
                    _ => AutoUpdateStatus::Idle,
                };
                this.status = status;
                cx.notify();
            });
            return Ok(());
        };

        this.update(cx, |this, cx| {
            this.status = AutoUpdateStatus::Downloading {
                version: newer_version.clone(),
            };
            cx.notify();
        });

        let installer_dir = InstallerDir::new().await?;
        let target_path = Self::target_path(&installer_dir).await?;
        download_release(&target_path, fetched_release_data, client).await?;

        this.update(cx, |this, cx| {
            this.status = AutoUpdateStatus::Installing {
                version: newer_version.clone(),
            };
            cx.notify();
        });

        let new_binary_path = Self::install_release(installer_dir, target_path, cx).await?;
        if let Some(new_binary_path) = new_binary_path {
            cx.update(|cx| cx.set_restart_path(new_binary_path));
        }

        this.update(cx, |this, cx| {
            this.set_should_show_update_notification(true, cx)
                .detach_and_log_err(cx);
            this.status = AutoUpdateStatus::Updated {
                version: newer_version,
            };
            cx.notify();
        });
        Ok(())
    }

    fn check_if_fetched_version_is_newer(
        release_channel: ReleaseChannel,
        app_commit_sha: Result<Option<String>>,
        installed_version: Version,
        fetched_version: String,
        status: AutoUpdateStatus,
    ) -> Result<Option<VersionCheckType>> {
        let parsed_fetched_version = fetched_version.parse::<Version>();

        if let AutoUpdateStatus::Updated { version, .. } = status {
            match version {
                VersionCheckType::Sha(cached_version) => {
                    let should_download =
                        parsed_fetched_version.as_ref().ok().is_none_or(|version| {
                            version.build.as_str().rsplit('.').next()
                                != Some(&cached_version.full())
                        });
                    let newer_version = should_download
                        .then(|| VersionCheckType::Sha(AppCommitSha::new(fetched_version)));
                    return Ok(newer_version);
                }
                VersionCheckType::Semantic(cached_version) => {
                    return Self::check_if_fetched_version_is_newer_non_nightly(
                        cached_version,
                        parsed_fetched_version?,
                    );
                }
            }
        }

        match release_channel {
            ReleaseChannel::Nightly => {
                let should_download = app_commit_sha
                    .ok()
                    .flatten()
                    .map(|sha| {
                        parsed_fetched_version.as_ref().ok().is_none_or(|version| {
                            version.build.as_str().rsplit('.').next() != Some(&sha)
                        })
                    })
                    .unwrap_or(true);
                let newer_version = should_download
                    .then(|| VersionCheckType::Sha(AppCommitSha::new(fetched_version)));
                Ok(newer_version)
            }
            _ => Self::check_if_fetched_version_is_newer_non_nightly(
                installed_version,
                parsed_fetched_version?,
            ),
        }
    }

    fn check_dependencies() -> Result<()> {
        #[cfg(not(target_os = "windows"))]
        anyhow::ensure!(
            which::which("rsync").is_ok(),
            "Could not auto-update because the required rsync utility was not found."
        );
        Ok(())
    }

    async fn target_path(installer_dir: &InstallerDir) -> Result<PathBuf> {
        let filename = match OS {
            "macos" => anyhow::Ok("Hawk.dmg"),
            "linux" => Ok("hawk.tar.gz"),
            "windows" => Ok("Hawk.exe"),
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        }?;

        Ok(installer_dir.path().join(filename))
    }

    async fn install_release(
        installer_dir: InstallerDir,
        target_path: PathBuf,
        cx: &AsyncApp,
    ) -> Result<Option<PathBuf>> {
        #[cfg(test)]
        if let Some(test_install) =
            cx.try_read_global::<tests::InstallOverride, _>(|g, _| g.0.clone())
        {
            return test_install(target_path, cx);
        }
        match OS {
            "macos" => install_release_macos(&installer_dir, target_path, cx).await,
            "linux" => install_release_linux(&installer_dir, target_path, cx).await,
            "windows" => install_release_windows(target_path).await,
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        }
    }

    fn check_if_fetched_version_is_newer_non_nightly(
        mut installed_version: Version,
        fetched_version: Version,
    ) -> Result<Option<VersionCheckType>> {
        // For non-nightly releases, ignore build and pre-release fields as they're not provided by our endpoints right now.
        installed_version.build = semver::BuildMetadata::EMPTY;
        installed_version.pre = semver::Prerelease::EMPTY;
        let should_download = fetched_version > installed_version;
        let newer_version = should_download.then(|| VersionCheckType::Semantic(fetched_version));
        Ok(newer_version)
    }

    pub fn set_should_show_update_notification(
        &self,
        should_show: bool,
        cx: &App,
    ) -> Task<Result<()>> {
        cx.background_spawn(async move {
            if should_show {
                KEY_VALUE_STORE
                    .write_kvp(
                        SHOULD_SHOW_UPDATE_NOTIFICATION_KEY.to_string(),
                        "".to_string(),
                    )
                    .await?;
            } else {
                KEY_VALUE_STORE
                    .delete_kvp(SHOULD_SHOW_UPDATE_NOTIFICATION_KEY.to_string())
                    .await?;
            }
            Ok(())
        })
    }

    pub fn should_show_update_notification(&self, cx: &App) -> Task<Result<bool>> {
        cx.background_spawn(async move {
            Ok(KEY_VALUE_STORE
                .read_kvp(SHOULD_SHOW_UPDATE_NOTIFICATION_KEY)?
                .is_some())
        })
    }
}

fn normalize_sha256_digest(digest: &str) -> Option<String> {
    let trimmed = digest.trim().trim_start_matches("sha256:");
    (!trimmed.is_empty()).then(|| trimmed.to_ascii_lowercase())
}

fn parse_sha256sums(checksum_contents: &str, target_asset_name: &str) -> Option<String> {
    let normalized_target_name = target_asset_name.to_ascii_lowercase();

    for line in checksum_contents.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            continue;
        }

        let mut parts = trimmed_line.split_whitespace();
        let Some(checksum) = parts.next() else {
            continue;
        };
        let Some(path_or_file_name) = parts.next() else {
            continue;
        };

        let file_name = path_or_file_name
            .trim_start_matches('*')
            .rsplit('/')
            .next()
            .unwrap_or(path_or_file_name)
            .to_ascii_lowercase();

        if file_name == normalized_target_name {
            return normalize_sha256_digest(checksum);
        }
    }

    None
}

async fn verify_download_sha256(path: &Path, expected_sha256: &str) -> Result<()> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0_u8; 8192];

    loop {
        let bytes_read = file.read(&mut chunk).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&chunk[..bytes_read]);
    }

    let actual_sha256 = format!("{:x}", hasher.finalize());
    let expected_sha256 = expected_sha256.to_ascii_lowercase();
    anyhow::ensure!(
        actual_sha256 == expected_sha256,
        "checksum mismatch for {:?}: expected {expected_sha256}, got {actual_sha256}",
        path
    );
    Ok(())
}

async fn download_remote_server_binary(
    target_path: &PathBuf,
    release: ReleaseAsset,
    client: Arc<HttpClientWithUrl>,
) -> Result<()> {
    let expected_sha256 = release.sha256.clone();
    let temp = tempfile::Builder::new().tempfile_in(remote_servers_dir())?;
    let mut temp_file = File::create(&temp).await?;

    let mut response = client.get(&release.url, Default::default(), true).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "failed to download remote server release: {:?}",
        response.status()
    );
    smol::io::copy(response.body_mut(), &mut temp_file).await?;
    temp_file.sync_all().await?;
    drop(temp_file);

    if let Some(expected_sha256) = expected_sha256 {
        verify_download_sha256(temp.path(), &expected_sha256).await?;
    } else {
        log::warn!(
            "remote server asset {} has no SHA-256 checksum; proceeding without verification",
            release.name
        );
    }

    smol::fs::rename(&temp, &target_path).await?;

    Ok(())
}

async fn cleanup_remote_server_cache(
    platform_dir: &Path,
    keep_path: &Path,
    limit: usize,
) -> Result<()> {
    if limit == 0 {
        return Ok(());
    }

    let mut entries = smol::fs::read_dir(platform_dir).await?;
    let now = SystemTime::now();
    let mut candidates = Vec::new();

    while let Some(entry) = entries.next().await {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("gz")) {
            continue;
        }

        let mtime = if path == keep_path {
            now
        } else {
            smol::fs::metadata(&path)
                .await
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        };

        candidates.push((path, mtime));
    }

    if candidates.len() <= limit {
        return Ok(());
    }

    candidates.sort_by(|(path_a, time_a), (path_b, time_b)| {
        time_b.cmp(time_a).then_with(|| path_a.cmp(path_b))
    });

    for (index, (path, _)) in candidates.into_iter().enumerate() {
        if index < limit || path == keep_path {
            continue;
        }

        if let Err(error) = smol::fs::remove_file(&path).await {
            log::warn!(
                "Failed to remove old remote server archive {:?}: {}",
                path,
                error
            );
        }
    }

    Ok(())
}

async fn download_release(
    target_path: &Path,
    release: ReleaseAsset,
    client: Arc<HttpClientWithUrl>,
) -> Result<()> {
    let expected_sha256 = release.sha256.clone();
    let mut target_file = File::create(&target_path).await?;

    let mut response = client.get(&release.url, Default::default(), true).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "failed to download update: {:?}",
        response.status()
    );
    smol::io::copy(response.body_mut(), &mut target_file).await?;
    target_file.sync_all().await?;
    drop(target_file);

    if let Some(expected_sha256) = expected_sha256 {
        verify_download_sha256(target_path, &expected_sha256).await?;
    } else {
        log::warn!(
            "release asset {} has no SHA-256 checksum; proceeding without verification",
            release.name
        );
    }

    log::info!("downloaded update. path:{:?}", target_path);

    Ok(())
}

async fn install_release_linux(
    temp_dir: &InstallerDir,
    downloaded_tar_gz: PathBuf,
    cx: &AsyncApp,
) -> Result<Option<PathBuf>> {
    let channel = cx.update(|cx| ReleaseChannel::global(cx).dev_name());
    let home_dir = PathBuf::from(env::var("HOME").context("no HOME env var set")?);
    let running_app_path = cx.update(|cx| cx.app_path())?;

    let extracted = temp_dir.path().join("hawk");
    fs::create_dir_all(&extracted)
        .await
        .context("failed to create directory into which to extract update")?;

    let output = new_command("tar")
        .arg("-xzf")
        .arg(&downloaded_tar_gz)
        .arg("-C")
        .arg(&extracted)
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to extract {:?} to {:?}: {:?}",
        downloaded_tar_gz,
        extracted,
        String::from_utf8_lossy(&output.stderr)
    );

    let suffix = if channel != "stable" {
        format!("-{}", channel)
    } else {
        String::default()
    };
    let app_folder_name = format!("hawk{}.app", suffix);

    let from = extracted.join(&app_folder_name);
    let mut to = home_dir.join(".local");

    let expected_suffix = format!("{}/libexec/hawk-editor", app_folder_name);

    if let Some(prefix) = running_app_path
        .to_str()
        .and_then(|str| str.strip_suffix(&expected_suffix))
    {
        to = PathBuf::from(prefix);
    }

    let output = new_command("rsync")
        .args(["-av", "--delete"])
        .arg(&from)
        .arg(&to)
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to copy Hawk update from {:?} to {:?}: {:?}",
        from,
        to,
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(Some(to.join(expected_suffix)))
}

async fn install_release_macos(
    temp_dir: &InstallerDir,
    downloaded_dmg: PathBuf,
    cx: &AsyncApp,
) -> Result<Option<PathBuf>> {
    let running_app_path = cx.update(|cx| cx.app_path())?;
    let running_app_filename = running_app_path
        .file_name()
        .with_context(|| format!("invalid running app path {running_app_path:?}"))?;

    let mount_path = temp_dir.path().join("Hawk");
    let mut mounted_app_path: OsString = mount_path.join(running_app_filename).into();

    mounted_app_path.push("/");
    let output = new_command("hdiutil")
        .args(["attach", "-nobrowse"])
        .arg(&downloaded_dmg)
        .arg("-mountroot")
        .arg(temp_dir.path())
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to mount: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Create an MacOsUnmounter that will be dropped (and thus unmount the disk) when this function exits
    let _unmounter = MacOsUnmounter {
        mount_path: mount_path.clone(),
        background_executor: cx.background_executor(),
    };

    let output = new_command("rsync")
        .args(["-av", "--delete", "--exclude", "Icon?"])
        .arg(&mounted_app_path)
        .arg(&running_app_path)
        .output()
        .await?;

    anyhow::ensure!(
        output.status.success(),
        "failed to copy app: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(None)
}

async fn cleanup_windows() -> Result<()> {
    let parent = std::env::current_exe()?
        .parent()
        .context("No parent dir for Hawk.exe")?
        .to_owned();

    // keep in sync with crates/auto_update_helper/src/updater.rs
    _ = smol::fs::remove_dir(parent.join("updates")).await;
    _ = smol::fs::remove_dir(parent.join("install")).await;
    _ = smol::fs::remove_dir(parent.join("old")).await;

    Ok(())
}

async fn install_release_windows(downloaded_installer: PathBuf) -> Result<Option<PathBuf>> {
    let output = new_command(downloaded_installer)
        .arg("/verysilent")
        .arg("/update=true")
        .arg("!desktopicon")
        .arg("!quicklaunchicon")
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "failed to start installer: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    // We return the path to the update helper program, because it will
    // perform the final steps of the update process, copying the new binary,
    // deleting the old one, and launching the new binary.
    let helper_path = std::env::current_exe()?
        .parent()
        .context("No parent dir for Hawk.exe")?
        .join("tools")
        .join("auto_update_helper.exe");
    Ok(Some(helper_path))
}

pub async fn finalize_auto_update_on_quit() {
    let Some(installer_path) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("updates")))
    else {
        return;
    };

    // The installer will create a flag file after it finishes updating
    let flag_file = installer_path.join("versions.txt");
    if flag_file.exists()
        && let Some(helper) = installer_path
            .parent()
            .map(|p| p.join("tools").join("auto_update_helper.exe"))
    {
        let mut command = util::command::new_command(helper);
        command.arg("--launch");
        command.arg("false");
        if let Ok(mut cmd) = command.spawn() {
            _ = cmd.status().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use client::Client;
    use clock::FakeSystemClock;
    use futures::channel::oneshot;
    use gpui::TestAppContext;
    use http_client::{FakeHttpClient, Response};
    use settings::default_settings;
    use std::{
        rc::Rc,
        sync::{
            Arc,
            atomic::{self, AtomicBool},
        },
    };
    use tempfile::tempdir;

    #[ctor::ctor]
    fn init_logger() {
        zlog::init_test();
    }

    use super::*;

    pub(super) struct InstallOverride(
        pub Rc<dyn Fn(PathBuf, &AsyncApp) -> Result<Option<PathBuf>>>,
    );
    impl Global for InstallOverride {}

    #[gpui::test]
    fn test_auto_update_defaults_to_true(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut store = SettingsStore::new(cx, &settings::default_settings());
            store
                .set_default_settings(&default_settings(), cx)
                .expect("Unable to set default settings");
            store
                .set_user_settings("{}", cx)
                .expect("Unable to set user settings");
            cx.set_global(store);
            assert!(AutoUpdateSetting::get_global(cx).0);
        });
    }

    #[gpui::test]
    async fn test_auto_update_downloads(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        zlog::init_test();
        let release_available = Arc::new(AtomicBool::new(false));

        let (dmg_tx, dmg_rx) = oneshot::channel::<String>();

        cx.update(|cx| {
            settings::init(cx);

            let current_version = semver::Version::new(0, 100, 0);
            release_channel::init_test(current_version, ReleaseChannel::Stable, cx);

            let clock = Arc::new(FakeSystemClock::new());
            let release_available = Arc::clone(&release_available);
            let dmg_rx = Arc::new(parking_lot::Mutex::new(Some(dmg_rx)));
            let fake_client_http = FakeHttpClient::create(move |req| {
                let release_available = release_available.load(atomic::Ordering::Relaxed);
                let dmg_rx = dmg_rx.clone();
                async move {
                if req.uri().path() == "/releases/stable/latest/asset" {
                    if release_available {
                        return Ok(Response::builder().status(200).body(
                            r#"{"version":"0.100.1","url":"https://test.example/new-download"}"#.into()
                        ).unwrap());
                    } else {
                        return Ok(Response::builder().status(200).body(
                            r#"{"version":"0.100.0","url":"https://test.example/old-download"}"#.into()
                        ).unwrap());
                    }
                } else if req.uri().path() == "/new-download" {
                    return Ok(Response::builder().status(200).body({
                        let dmg_rx = dmg_rx.lock().take().unwrap();
                        dmg_rx.await.unwrap().into()
                    }).unwrap());
                }
                Ok(Response::builder().status(404).body("".into()).unwrap())
                }
            });
            let client = Client::new(clock, fake_client_http, cx);
            crate::init(client, cx);
        });

        let auto_updater = cx.update(|cx| AutoUpdater::get(cx).expect("auto updater should exist"));

        cx.background_executor.run_until_parked();

        auto_updater.read_with(cx, |updater, _| {
            assert_eq!(updater.status(), AutoUpdateStatus::Idle);
            assert_eq!(updater.current_version(), semver::Version::new(0, 100, 0));
        });

        release_available.store(true, atomic::Ordering::SeqCst);
        cx.background_executor.advance_clock(POLL_INTERVAL);
        cx.background_executor.run_until_parked();

        loop {
            cx.background_executor.timer(Duration::from_millis(0)).await;
            cx.run_until_parked();
            let status = auto_updater.read_with(cx, |updater, _| updater.status());
            if !matches!(status, AutoUpdateStatus::Idle) {
                break;
            }
        }
        let status = auto_updater.read_with(cx, |updater, _| updater.status());
        assert_eq!(
            status,
            AutoUpdateStatus::Downloading {
                version: VersionCheckType::Semantic(semver::Version::new(0, 100, 1))
            }
        );

        dmg_tx.send("<fake-zed-update>".to_owned()).unwrap();

        let tmp_dir = Arc::new(tempdir().unwrap());

        cx.update(|cx| {
            let tmp_dir = tmp_dir.clone();
            cx.set_global(InstallOverride(Rc::new(move |target_path, _cx| {
                let tmp_dir = tmp_dir.clone();
                let dest_path = tmp_dir.path().join("hawk");
                std::fs::copy(&target_path, &dest_path)?;
                Ok(Some(dest_path))
            })));
        });

        loop {
            cx.background_executor.timer(Duration::from_millis(0)).await;
            cx.run_until_parked();
            let status = auto_updater.read_with(cx, |updater, _| updater.status());
            if !matches!(status, AutoUpdateStatus::Downloading { .. }) {
                break;
            }
        }
        let status = auto_updater.read_with(cx, |updater, _| updater.status());
        assert_eq!(
            status,
            AutoUpdateStatus::Updated {
                version: VersionCheckType::Semantic(semver::Version::new(0, 100, 1))
            }
        );
        let will_restart = cx.expect_restart();
        cx.update(|cx| cx.restart());
        let path = will_restart.await.unwrap().unwrap();
        assert_eq!(path, tmp_dir.path().join("hawk"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "<fake-zed-update>");
    }

    #[test]
    fn test_stable_does_not_update_when_fetched_version_is_not_higher() {
        let release_channel = ReleaseChannel::Stable;
        let app_commit_sha = Ok(Some("a".to_string()));
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Idle;
        let fetched_version = semver::Version::new(1, 0, 0);

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_version.to_string(),
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_stable_does_update_when_fetched_version_is_higher() {
        let release_channel = ReleaseChannel::Stable;
        let app_commit_sha = Ok(Some("a".to_string()));
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Idle;
        let fetched_version = semver::Version::new(1, 0, 1);

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_version.to_string(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Semantic(fetched_version))
        );
    }

    #[test]
    fn test_stable_does_not_update_when_fetched_version_is_not_higher_than_cached() {
        let release_channel = ReleaseChannel::Stable;
        let app_commit_sha = Ok(Some("a".to_string()));
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Semantic(semver::Version::new(1, 0, 1)),
        };
        let fetched_version = semver::Version::new(1, 0, 1);

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_version.to_string(),
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_stable_does_update_when_fetched_version_is_higher_than_cached() {
        let release_channel = ReleaseChannel::Stable;
        let app_commit_sha = Ok(Some("a".to_string()));
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Semantic(semver::Version::new(1, 0, 1)),
        };
        let fetched_version = semver::Version::new(1, 0, 2);

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_version.to_string(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Semantic(fetched_version))
        );
    }

    #[test]
    fn test_nightly_does_not_update_when_fetched_sha_is_same() {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(Some("a".to_string()));
        let mut installed_version = semver::Version::new(1, 0, 0);
        installed_version.build = semver::BuildMetadata::new("a").unwrap();
        let status = AutoUpdateStatus::Idle;
        let fetched_sha = "1.0.0+a".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha,
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_nightly_does_update_when_fetched_sha_is_not_same() {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(Some("a".to_string()));
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Idle;
        let fetched_sha = "b".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha.clone(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Sha(AppCommitSha::new(fetched_sha)))
        );
    }

    #[test]
    fn test_nightly_does_not_update_when_fetched_version_is_same_as_cached() {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(Some("a".to_string()));
        let mut installed_version = semver::Version::new(1, 0, 0);
        installed_version.build = semver::BuildMetadata::new("a").unwrap();
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Sha(AppCommitSha::new("b".to_string())),
        };
        let fetched_sha = "1.0.0+b".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha,
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_nightly_does_update_when_fetched_sha_is_not_same_as_cached() {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(Some("a".to_string()));
        let mut installed_version = semver::Version::new(1, 0, 0);
        installed_version.build = semver::BuildMetadata::new("a").unwrap();
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Sha(AppCommitSha::new("b".to_string())),
        };
        let fetched_sha = "1.0.0+c".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha.clone(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Sha(AppCommitSha::new(fetched_sha)))
        );
    }

    #[test]
    fn test_nightly_does_update_when_installed_versions_sha_cannot_be_retrieved() {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(None);
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Idle;
        let fetched_sha = "a".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha.clone(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Sha(AppCommitSha::new(fetched_sha)))
        );
    }

    #[test]
    fn test_nightly_does_not_update_when_cached_update_is_same_as_fetched_and_installed_versions_sha_cannot_be_retrieved()
     {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(None);
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Sha(AppCommitSha::new("b".to_string())),
        };
        let fetched_sha = "1.0.0+b".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha,
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_nightly_does_update_when_cached_update_is_not_same_as_fetched_and_installed_versions_sha_cannot_be_retrieved()
     {
        let release_channel = ReleaseChannel::Nightly;
        let app_commit_sha = Ok(None);
        let installed_version = semver::Version::new(1, 0, 0);
        let status = AutoUpdateStatus::Updated {
            version: VersionCheckType::Sha(AppCommitSha::new("b".to_string())),
        };
        let fetched_sha = "c".to_string();

        let newer_version = AutoUpdater::check_if_fetched_version_is_newer(
            release_channel,
            app_commit_sha,
            installed_version,
            fetched_sha.clone(),
            status,
        );

        assert_eq!(
            newer_version.unwrap(),
            Some(VersionCheckType::Sha(AppCommitSha::new(fetched_sha)))
        );
    }

    #[test]
    fn test_parse_sha256sums_finds_named_asset() {
        let checksums = "\
1234abcd  hawk-linux-aarch64.tar.gz
deadbeef  hawk-linux-x86_64.tar.gz
";
        let parsed_checksum = parse_sha256sums(checksums, "hawk-linux-x86_64.tar.gz");

        assert_eq!(parsed_checksum, Some("deadbeef".to_string()));
    }

    #[test]
    fn test_parse_sha256sums_supports_star_prefix() {
        let checksums = "\
1234abcd *hawk-linux-x86_64.tar.gz
";
        let parsed_checksum = parse_sha256sums(checksums, "hawk-linux-x86_64.tar.gz");

        assert_eq!(parsed_checksum, Some("1234abcd".to_string()));
    }

    #[test]
    fn test_normalize_sha256_digest_removes_algorithm_prefix() {
        let digest = "sha256:ABCDEF";
        assert_eq!(normalize_sha256_digest(digest), Some("abcdef".to_string()));
    }
}
