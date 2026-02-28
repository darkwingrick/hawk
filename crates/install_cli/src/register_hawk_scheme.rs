use client::HAWK_URL_SCHEME;
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the hawk:// URL scheme handler.
        RegisterHawkScheme
    ]
);

pub async fn register_hawk_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    cx.update(|cx| cx.register_url_scheme(HAWK_URL_SCHEME)).await
}
