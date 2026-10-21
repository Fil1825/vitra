#!/bin/sh
# Type-checks the app crate for macOS and Windows from Linux with clippy (no linking), so
# platform-specific code (#[cfg(target_os = ...)]) is verified without those machines.
set -eu
here=$(cd "$(dirname "$0")" && pwd)

# tauri-build embeds the Windows icon with llvm-rc. Distro LLVM may only ship it versioned
# (/usr/lib/llvm-N/bin) without the /usr/bin symlink from the `llvm` package.
if ! command -v llvm-rc >/dev/null 2>&1; then
  rc=$(ls /usr/lib/llvm-*/bin/llvm-rc 2>/dev/null | sort -V | tail -n 1)
  if [ -z "$rc" ]; then
    echo "llvm-rc not found: install LLVM (e.g. sudo apt-get install llvm)" >&2
    exit 1
  fi
  export RC_x86_64_pc_windows_msvc="$rc"
fi
for target in aarch64-apple-darwin x86_64-pc-windows-msvc; do
  rustup target add "$target" >/dev/null 2>&1
  echo "== $target"
  # No Apple or MSVC toolchain here: C code (bundled SQLite, ...) "compiles" to empty files.
  CC_aarch64_apple_darwin="$here/fake-cc" AR_aarch64_apple_darwin=ar \
    CC_x86_64_pc_windows_msvc="$here/fake-cc" AR_x86_64_pc_windows_msvc="$here/fake-cc" \
    cargo clippy -p vitra --target "$target" --all-targets -- -D warnings
done
