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
cp packaging/linux/magpie.svg packaging/linux/magpie-16.svg packaging/linux/magpie-24.svg "$STAGE/"
cp LICENSE README.md THIRD-PARTY-LICENSES.txt "$STAGE/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
echo "Built dist/$NAME.tar.gz"
