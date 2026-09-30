#!/bin/sh
# Magpie installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/ahaan-shah/magpie/main/install.sh | sh
#
# Environment:
#   MAGPIE_VERSION      install a specific version (e.g. 0.1.0) instead of the latest
#   MAGPIE_INSTALL_DIR  where the Linux binary goes (default: ~/.local/bin)
#   MAGPIE_BASE_URL     download from a mirror instead of GitHub releases
set -eu

REPO="ahaan-shah/magpie"
VERSION="${MAGPIE_VERSION:-latest}"

say() { printf '\033[1;35mmagpie\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "this installer needs '$1'"; }

need curl
need uname

if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
else
    BASE="https://github.com/$REPO/releases/download/v${VERSION#v}"
fi

BASE="${MAGPIE_BASE_URL:-$BASE}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

fetch() {
    say "downloading $1"
    curl -fL --progress-bar "$BASE/$1" -o "$TMP/$1" || die "couldn't download $BASE/$1"
}

verify() {
    if curl -fsL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" 2>/dev/null; then
        expected="$(grep " $1\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
        [ -n "$expected" ] || return 0
        if command -v sha256sum >/dev/null 2>&1; then
            actual="$(sha256sum "$TMP/$1" | cut -d' ' -f1)"
        else
            actual="$(shasum -a 256 "$TMP/$1" | cut -d' ' -f1)"
        fi
        [ "$expected" = "$actual" ] || die "checksum mismatch for $1"
        say "checksum ok"
    fi
}

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
Linux)
    case "$ARCH" in
    x86_64 | amd64) ARCH=x86_64 ;;
    aarch64 | arm64) ARCH=aarch64 ;;
    *) die "unsupported architecture: $ARCH (try: cargo install magpie-finance)" ;;
    esac
    need tar
    ASSET="magpie-$ARCH-unknown-linux-gnu.tar.gz"
    fetch "$ASSET"
    verify "$ASSET"
    tar -xzf "$TMP/$ASSET" -C "$TMP"
    SRC="$TMP/magpie-$ARCH-unknown-linux-gnu"

    BIN_DIR="${MAGPIE_INSTALL_DIR:-$HOME/.local/bin}"
    DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
    mkdir -p "$BIN_DIR" "$DATA/applications" "$DATA/icons/hicolor/512x512/apps"
    install -m 755 "$SRC/magpie" "$BIN_DIR/magpie"
    install -m 644 "$SRC/magpie.png" "$DATA/icons/hicolor/512x512/apps/magpie.png"
    sed "s|^Exec=magpie|Exec=$BIN_DIR/magpie|" "$SRC/magpie.desktop" >"$DATA/applications/magpie.desktop"
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA/applications" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$DATA/icons/hicolor" 2>/dev/null || true

    say "installed $BIN_DIR/magpie"
    case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "note: $BIN_DIR isn't on your PATH — add it, or launch Magpie from your app menu" ;;
    esac
    say "run it with: magpie"
    ;;
Darwin)
    need unzip
    ASSET="Magpie-macos-universal.zip"
    fetch "$ASSET"
    verify "$ASSET"
    unzip -q "$TMP/$ASSET" -d "$TMP"
    if [ -w /Applications ]; then DEST=/Applications; else DEST="$HOME/Applications"; mkdir -p "$DEST"; fi
    rm -rf "$DEST/Magpie.app"
    mv "$TMP/Magpie.app" "$DEST/"
    # Downloaded via curl, so there's no quarantine flag — but clear it just in case.
    xattr -dr com.apple.quarantine "$DEST/Magpie.app" 2>/dev/null || true
    say "installed $DEST/Magpie.app"
    say "open it from Launchpad, or run: open -a Magpie"
    ;;
*)
    die "unsupported OS: $OS (Magpie supports Linux and macOS)"
    ;;
esac
