#!/bin/sh
# Magpie installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/ahaan-shah/magpie/main/install.sh | sh
#
# To uninstall (your data is kept):
#
#   curl -fsSL https://raw.githubusercontent.com/ahaan-shah/magpie/main/install.sh | sh -s -- --uninstall
#
# Environment:
#   MAGPIE_VERSION      install a specific version (e.g. 0.1.0) instead of the latest
#   MAGPIE_INSTALL_DIR  where the Linux binary goes (default: ~/.local/bin)
#   MAGPIE_BASE_URL     download from a mirror instead of GitHub releases
set -eu

REPO="ahaan-shah/magpie"
VERSION="${MAGPIE_VERSION:-latest}"

# A terminal gets colour and a drawn progress bar; a pipe or a log gets plain lines.
FANCY=""
if [ -t 1 ] && [ "${TERM:-dumb}" != dumb ]; then FANCY=1; fi
TRUECOLOR=""
if [ -n "$FANCY" ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD="$(printf '\033[1m')" DIM="$(printf '\033[2m')" RED="$(printf '\033[31m')" RESET="$(printf '\033[0m')"
    # Magpie's accent, the orange of its icon.
    case "${COLORTERM:-}" in
    truecolor | 24bit) ACCENT="$(printf '\033[38;2;228;129;79m')" TRUECOLOR=1 ;;
    *) case "${TERM:-}" in *256color*) ACCENT="$(printf '\033[38;5;173m')" ;; *) ACCENT="$(printf '\033[33m')" ;; esac ;;
    esac
else
    BOLD="" DIM="" RED="" RESET="" ACCENT=""
fi

if [ -n "$FANCY" ]; then
    say() { printf '  %s\n' "$*"; }
    ok() { printf '  %s✓%s %s\n' "$ACCENT" "$RESET" "$*"; }
    die() { printf '\r\033[K\033[?25h  %serror%s %s\n' "$BOLD$RED" "$RESET" "$*" >&2; exit 1; }
else
    say() { printf 'magpie: %s\n' "$*"; }
    ok() { say "$*"; }
    die() { printf 'magpie: error: %s\n' "$*" >&2; exit 1; }
fi
need() { command -v "$1" >/dev/null 2>&1 || die "this installer needs '$1'"; }

# "MAGPIE" in big block letters, Magpie's orange, a shade lighter at the top.
# Drawn from a mask (# = a block, . = a gap) so every row comes out the same
# width: printf can't pad multibyte strings, and editors strip trailing spaces.
# A terminal narrower than the letters gets the plain word.
banner() {
    [ -n "$FANCY" ] || return 0
    printf '\n'
    if [ "$(tput cols 2>/dev/null || echo 80)" -lt 58 ]; then
        printf '  %sMagpie%s\n  %sinstaller%s\n\n' "$BOLD$ACCENT" "$RESET" "$DIM" "$RESET"
        return 0
    fi
    i=0
    while IFS= read -r row; do
        colour="$ACCENT"
        if [ -n "$TRUECOLOR" ]; then
            colour="$(printf '\033[38;2;%d;%d;%dm' $((246 - i * 40 / 4)) $((156 - i * 56 / 4)) $((106 - i * 56 / 4)))"
        fi
        row="$(printf '%s' "$row" | sed 's/#/█/g; s/\./ /g')"
        printf '  %s%s%s\n' "$colour" "$row" "$RESET"
        i=$((i + 1))
    done <<'LETTERS'
##......##....####......######..#######...##..########
####..####..##....##..##........##....##..##..##......
##..##..##..########..##..####..#######...##..######..
##......##..##....##..##....##..##........##..##......
##......##..##....##....######..##........##..########
LETTERS
    printf '\n  %sinstaller%s\n\n' "$DIM" "$RESET"
}

# "~/.local/bin" reads better than the full path.
tilde() {
    case "$1" in
    "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;;
    *) printf '%s' "$1" ;;
    esac
}

OS="$(uname -s)"
BIN_DIR="${MAGPIE_INSTALL_DIR:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
ICONS="$DATA/icons/hicolor"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA/applications" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
}

# Removes exactly what an install put there. The data folder stays, so
# reinstalling picks up where you left off.
uninstall() {
    case "$OS" in
    Linux)
        [ -e "$BIN_DIR/magpie" ] || [ -e "$DATA/applications/magpie.desktop" ] ||
            die "Magpie isn't installed in $BIN_DIR (set MAGPIE_INSTALL_DIR if you chose another folder)"
        rm -f "$BIN_DIR/magpie" "$DATA/applications/magpie.desktop" \
            "$ICONS"/*/apps/magpie.svg "$ICONS"/*/apps/magpie.png
        # Left only if an in-app update was interrupted.
        rm -rf "$BIN_DIR"/.magpie-update-* "$BIN_DIR/.magpie.old"
        refresh
        say "Uninstalled $BIN_DIR/magpie"
        say "Your data is still in $DATA/magpie (delete it to remove that too)"
        ;;
    Darwin)
        found=""
        for app in /Applications/Magpie.app "$HOME/Applications/Magpie.app"; do
            [ -e "$app" ] || continue
            found=1
            rm -rf "$app" 2>/dev/null || die "couldn't remove $app (try: sudo rm -rf '$app')"
            say "Uninstalled $app"
        done
        [ -n "$found" ] || die "Magpie.app isn't in /Applications or ~/Applications"
        say "Your data is still in ~/Library/Application Support/dev.magpie.magpie (delete it to remove that too)"
        ;;
    *)
        die "unsupported OS: $OS (Magpie supports Linux and macOS)"
        ;;
    esac
}

case "${1:-}" in
"") ;;
--uninstall)
    uninstall
    exit 0
    ;;
*) die "unknown option: $1 (the only option is --uninstall)" ;;
esac

need curl
need uname
need awk

# Name the version up front (and download exactly that one) by following
# GitHub's redirect from /releases/latest to its tag.
if [ "$VERSION" = "latest" ] && [ -z "${MAGPIE_BASE_URL:-}" ]; then
    tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null || true)"
    case "$tag" in */tag/v*) VERSION="${tag##*/tag/v}" ;; esac
fi
if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
    SHOWN="Magpie"
else
    VERSION="${VERSION#v}"
    BASE="https://github.com/$REPO/releases/download/v$VERSION"
    SHOWN="Magpie $VERSION"
fi
BASE="${MAGPIE_BASE_URL:-$BASE}"

TMP="$(mktemp -d)"
PID=""
cleanup() {
    [ -n "$PID" ] && kill "$PID" 2>/dev/null
    [ -n "$FANCY" ] && printf '\033[?25h'
    rm -rf "$TMP"
}
trap cleanup EXIT
trap 'cleanup; trap - EXIT; exit 130' INT TERM

mb() {
    tenths=$(($1 * 10 / 1048576))
    printf '%d.%d' $((tenths / 10)) $((tenths % 10))
}

# One line of the bar: ■ for what's in, · for what's left, then % and MB.
draw() {
    got=0
    if [ -f "$TMP/$1" ]; then got="$(wc -c <"$TMP/$1" | tr -d ' ')"; fi
    # Redirects come first in the headers; the last length is the file's.
    total="$(tr -d '\r' <"$TMP/headers" 2>/dev/null | awk 'tolower($1) == "content-length:" { n = $2 } END { print n + 0 }')"
    cols="$(tput cols 2>/dev/null || echo 80)"
    width=32
    [ "$cols" -lt 70 ] && width=$((cols - 38))
    [ "$width" -lt 10 ] && width=10
    if [ "$total" -gt 0 ]; then
        [ "$got" -gt "$total" ] && got="$total"
        filled=$((got * width / total))
        pct="$((got * 100 / total))%"
        size="$(mb "$got") / $(mb "$total") MB"
    else
        filled=0 pct="" size="$(mb "$got") MB"
    fi
    done_part="" left_part="" n=0
    while [ $n -lt $width ]; do
        if [ $n -lt $filled ]; then done_part="$done_part■"; else left_part="$left_part·"; fi
        n=$((n + 1))
    done
    printf '\r\033[K  %s%s%s%s%s  %4s  %s%s%s' "$ACCENT" "$done_part" "$RESET$DIM" "$left_part" "$RESET" "$pct" "$DIM" "$size" "$RESET"
}

# Downloads $1 into $TMP; `bar` draws the progress bar in a terminal.
fetch() {
    if [ -z "$FANCY" ] || [ "${2:-}" != bar ]; then
        [ -n "$FANCY" ] || say "Downloading $1"
        curl -fsSL "$BASE/$1" -o "$TMP/$1" || die "couldn't download $BASE/$1"
        return 0
    fi
    : >"$TMP/headers"
    printf '\033[?25l'
    curl -fsSL -D "$TMP/headers" "$BASE/$1" -o "$TMP/$1" 2>/dev/null &
    PID=$!
    while kill -0 "$PID" 2>/dev/null; do
        draw "$1"
        sleep 0.1 2>/dev/null || sleep 1
    done
    if wait "$PID"; then status=0; else status=$?; fi
    PID=""
    [ "$status" -eq 0 ] || die "couldn't download $BASE/$1"
    draw "$1"
    printf '\033[?25h\n\n'
}

start() {
    banner
    if [ -n "$FANCY" ]; then
        printf '  %sInstalling%s %s%s%s\n\n' "$DIM" "$RESET" "$BOLD" "$SHOWN" "$RESET"
    else
        say "Installing $SHOWN"
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

# Checked whenever the release lists the file in SHA256SUMS.
verify() {
    if curl -fsL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" 2>/dev/null; then
        expected="$(grep " $1\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
        [ -n "$expected" ] || return 0
        [ "$expected" = "$(sha256 "$TMP/$1")" ] || die "checksum mismatch for $1"
        ok "Checksum verified"
    fi
}

ARCH="$(uname -m)"

case "$OS" in
Linux)
    case "$ARCH" in
    x86_64 | amd64) ARCH=x86_64 ;;
    aarch64 | arm64) ARCH=aarch64 ;;
    *) die "unsupported architecture: $ARCH (you can build from source: https://github.com/$REPO#building-from-source)" ;;
    esac
    need tar
    ASSET="magpie-$ARCH-unknown-linux-gnu.tar.gz"
    start
    fetch "$ASSET" bar
    verify "$ASSET"
    tar -xzf "$TMP/$ASSET" -C "$TMP"
    SRC="$TMP/magpie-$ARCH-unknown-linux-gnu"

    mkdir -p "$BIN_DIR" "$DATA/applications"
    install -m 755 "$SRC/magpie" "$BIN_DIR/magpie"
    if [ -f "$SRC/magpie.svg" ]; then
        # Vector icons only: sharp at any size and any display scale. Remove
        # PNGs from older installs so icon lookups can't pick a bitmap and
        # upscale it on HiDPI screens.
        rm -f "${ICONS:?}"/*/apps/magpie.png
        mkdir -p "$ICONS/scalable/apps"
        install -m 644 "$SRC/magpie.svg" "$ICONS/scalable/apps/magpie.svg"
        for s in 16 24; do
            [ -f "$SRC/magpie-$s.svg" ] || continue
            mkdir -p "$ICONS/${s}x${s}/apps"
            install -m 644 "$SRC/magpie-$s.svg" "$ICONS/${s}x${s}/apps/magpie.svg"
        done
    else
        mkdir -p "$ICONS/512x512/apps"
        install -m 644 "$SRC/magpie.png" "$ICONS/512x512/apps/magpie.png"
    fi
    sed "s|^Exec=magpie|Exec=$BIN_DIR/magpie|" "$SRC/magpie.desktop" >"$DATA/applications/magpie.desktop"
    refresh

    ok "Installed to $(tilde "$BIN_DIR")"
    [ -n "$FANCY" ] && printf '\n'
    case ":$PATH:" in
    *":$BIN_DIR:"*) say "Run it with ${BOLD}magpie${RESET}, or open it from your app menu" ;;
    *) say "$(tilde "$BIN_DIR") isn't on your PATH: add it, or open Magpie from your app menu" ;;
    esac
    [ -n "$FANCY" ] && printf '\n'
    true
    ;;
Darwin)
    need unzip
    ASSET="Magpie-macos-universal.zip"
    start
    fetch "$ASSET" bar
    verify "$ASSET"
    unzip -q "$TMP/$ASSET" -d "$TMP"
    if [ -w /Applications ]; then DEST=/Applications; else DEST="$HOME/Applications"; mkdir -p "$DEST"; fi
    rm -rf "$DEST/Magpie.app"
    mv "$TMP/Magpie.app" "$DEST/"
    # Downloaded via curl, so there's no quarantine flag, but clear it just in case.
    xattr -dr com.apple.quarantine "$DEST/Magpie.app" 2>/dev/null || true
    ok "Installed to $(tilde "$DEST")"
    [ -n "$FANCY" ] && printf '\n'
    say "Open it from Launchpad, or run ${BOLD}open -a Magpie${RESET}"
    [ -n "$FANCY" ] && printf '\n'
    true
    ;;
*)
    die "unsupported OS: $OS (Magpie supports Linux and macOS)"
    ;;
esac
