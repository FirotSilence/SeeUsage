#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/SeeUsage.app"
CONTENTS="$APP/Contents"
TARGET="aarch64-apple-darwin"

append_rustflag() {
  local flag="$1"
  if [[ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]]; then
    CARGO_ENCODED_RUSTFLAGS+=$'\x1f'
  fi
  CARGO_ENCODED_RUSTFLAGS+="$flag"
  export CARGO_ENCODED_RUSTFLAGS
}

[[ -n "${HOME:-}" ]] && append_rustflag "--remap-path-prefix=$HOME=user-home"
[[ -n "${CARGO_HOME:-}" ]] && append_rustflag "--remap-path-prefix=$CARGO_HOME=cargo-home"
[[ -n "${RUSTUP_HOME:-}" ]] && append_rustflag "--remap-path-prefix=$RUSTUP_HOME=rustup-home"
append_rustflag "--remap-path-prefix=$ROOT=seeusage-src"

cd "$ROOT"
cargo build --release --locked --target "$TARGET"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources/assets"
cp "target/$TARGET/release/SeeUsage" "$CONTENTS/MacOS/SeeUsage"
cp packaging/macos/Info.plist "$CONTENTS/Info.plist"
cp README.md README.zh-CN.md LICENSE NOTICE THIRD_PARTY_NOTICES.md THIRD_PARTY_DEPENDENCIES.md "$CONTENTS/Resources/"
cp assets/seeusage-preview.png "$CONTENTS/Resources/assets/"
chmod +x "$CONTENTS/MacOS/SeeUsage"

mkdir -p "$ROOT/dist"
rm -f "$ROOT/dist/SeeUsage-macos-arm64.zip" "$ROOT/dist/SeeUsage-macos-arm64.zip.sha256"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ROOT/dist/SeeUsage-macos-arm64.zip"
(
  cd "$ROOT/dist"
  shasum -a 256 SeeUsage-macos-arm64.zip > SeeUsage-macos-arm64.zip.sha256
)
printf '%s\n' "$ROOT/dist/SeeUsage-macos-arm64.zip"
printf '%s\n' "$ROOT/dist/SeeUsage-macos-arm64.zip.sha256"
