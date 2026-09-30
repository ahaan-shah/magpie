#!/usr/bin/env bash
# Builds a release tarball for one Linux target:
#   ./scripts/package-linux.sh x86_64-unknown-linux-gnu
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET="${1:-$(rustc -vV | sed -n 's/host: //p')}"
cargo build --profile dist --locked --target "$TARGET" -p magpie-finance
NAME="magpie-$TARGET"
STAGE="dist/$NAME"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp "target/$TARGET/dist/magpie" "$STAGE/"
cp packaging/linux/magpie.desktop "$STAGE/"
cp packaging/linux/icon-512.png "$STAGE/magpie.png"
cp LICENSE README.md "$STAGE/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
echo "Built dist/$NAME.tar.gz"
