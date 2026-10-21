#!/bin/sh
# Type-checks the app crate for macOS and Windows from Linux with clippy (no linking), so
# platform-specific code (#[cfg(target_os = ...)]) is verified without those machines.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
for target in aarch64-apple-darwin x86_64-pc-windows-msvc; do
  rustup target add "$target" >/dev/null 2>&1
  echo "== $target"
  CC_aarch64_apple_darwin="$here/fake-cc" AR_aarch64_apple_darwin=ar \
    cargo clippy -p vitra --target "$target" --all-targets -- -D warnings
done
