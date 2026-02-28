#!/usr/bin/env sh
set -eu

# Uninstalls Hawk that was installed using the install.sh script

check_remaining_installations() {
    platform="$(uname -s)"
    if [ "$platform" = "Darwin" ]; then
        # Check for any Hawk variants in /Applications
        remaining=$(ls -d /Applications/Hawk*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    else
        # Check for any Hawk variants in ~/.local
        remaining=$(ls -d "$HOME/.local/hawk"*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    fi
}

prompt_remove_preferences() {
    printf "Do you want to keep your Hawk preferences? [Y/n] "
    read -r response
    case "$response" in
        [nN]|[nN][oO])
            rm -rf "$HOME/.config/hawk"
            echo "Preferences removed."
            ;;
        *)
            echo "Preferences kept."
            ;;
    esac
}

main() {
    platform="$(uname -s)"
    channel="${HAWK_CHANNEL:-stable}"

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    "$platform"

    echo "Hawk has been uninstalled"
}

linux() {
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    db_suffix="stable"
    case "$channel" in
      stable)
        appid="dev.hawk.Hawk"
        db_suffix="stable"
        ;;
      nightly)
        appid="dev.hawk.Hawk-Nightly"
        db_suffix="nightly"
        ;;
      preview)
        appid="dev.hawk.Hawk-Preview"
        db_suffix="preview"
        ;;
      dev)
        appid="dev.hawk.Hawk-Dev"
        db_suffix="dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.hawk.Hawk"
        db_suffix="stable"
        ;;
    esac

    # Remove the app directory
    rm -rf "$HOME/.local/hawk$suffix.app"

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/hawk"

    # Remove the .desktop file
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    # Remove the database directory for this channel
    rm -rf "$HOME/.local/share/hawk/db/0-$db_suffix"

    # Remove socket file
    rm -f "$HOME/.local/share/hawk/hawk-$db_suffix.sock"

    # Remove the entire Hawk directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/.local/share/hawk"
        prompt_remove_preferences
    fi

    rm -rf $HOME/.hawk_server
}

macos() {
    app="Hawk.app"
    db_suffix="stable"
    app_id="dev.hawk.Hawk"
    case "$channel" in
      nightly)
        app="Hawk Nightly.app"
        db_suffix="nightly"
        app_id="dev.hawk.Hawk-Nightly"
        ;;
      preview)
        app="Hawk Preview.app"
        db_suffix="preview"
        app_id="dev.hawk.Hawk-Preview"
        ;;
      dev)
        app="Hawk Dev.app"
        db_suffix="dev"
        app_id="dev.hawk.Hawk-Dev"
        ;;
    esac

    # Remove the app bundle
    if [ -d "/Applications/$app" ]; then
        rm -rf "/Applications/$app"
    fi

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/hawk"

    # Remove the database directory for this channel
    rm -rf "$HOME/Library/Application Support/Hawk/db/0-$db_suffix"

    # Remove app-specific files and directories
    rm -rf "$HOME/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments/$app_id.sfl"*
    rm -rf "$HOME/Library/Caches/$app_id"
    rm -rf "$HOME/Library/HTTPStorages/$app_id"
    rm -rf "$HOME/Library/Preferences/$app_id.plist"
    rm -rf "$HOME/Library/Saved Application State/$app_id.savedState"

    # Remove the entire Hawk directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/Library/Application Support/Hawk"
        rm -rf "$HOME/Library/Logs/Hawk"

        prompt_remove_preferences
    fi

    rm -rf $HOME/.hawk_server
}

main "$@"
