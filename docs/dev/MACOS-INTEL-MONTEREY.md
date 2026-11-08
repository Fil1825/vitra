# Intel macOS (Monterey+) and bundled mpv

## User-visible failure (v0.1.1)

On **Intel Macs running macOS 12 Monterey**, Vitra’s DMG showed macOS’s **prohibited** overlay on
`Vitra.app` when `LSMinimumSystemVersion` was **15.0** (Sequoia). That overlay is honest: the official
**mpv v0.41.0 macos-15-intel** sidecar’s main binary declares **minOS 15.0**, so the app could not run
on Monterey even if the plist were lowered.

## Bundled Intel mpv (from this branch)

| Item | Value |
|------|--------|
| Source | [stolendata djinn mac builds](https://laboratory.stolendata.net/~djinn/mpv_osx/) (linked from [mpv.io/installation](https://mpv.io/installation)) |
| Artifact | `mpv-0.39.0.tar.gz` |
| SHA256 | `35ec81ad86a97b24956a8d0f4fa1ba2690b44ae7741c920e923620bcd7bd402a` |
| mpv version | **0.39.0** (Info.plist / `--version`) |
| Architecture | thin **x86_64** |
| Mach-O minOS | **11.0** on `mpv` and bundled dylibs (Monterey **12.7** ceiling passes `mach-minos.py`) |
| Vitra Intel plist | **12.0** (`tauri.intel.conf.json`) — product floor for Monterey |

This is **not** an official `mpv-player` GitHub release. Support, security fixes, and reproducibility
depend on the third-party builder. GPLv2+ applies to mpv; Vitra is GPLv3.

### Tradeoffs vs official Intel mpv 0.41.0

- Vitra’s player expects mpv **≥ 0.39** for some IPC/lavfi options; **0.39.0** meets that floor.
- **0.41.0** may include fixes and features Vitra does not require yet; Intel builds stay one minor
  behind Apple Silicon until upstream ships an official Intel bundle with minOS ≤ 12.
- **Not runtime-verified here:** no physical Intel Mac in CI; Mach-O and packaging checks only.

### Audit notes (Linux unpack, Nov 2025)

- Checksum verified before extract.
- No `libmpv` — CLI bundle (`mpv.app` + adjacent dylibs), same integration model as upstream zip.
- Offline load closure: `scripts/release/mach-deps.py` parses `LC_LOAD_DYLIB` — all dynamic
  deps resolve under `@executable_path/lib/` (46 Mach-O files). **Stale builder RPATHs** (paths
  under `/Users/.../homebrew/...` on the main `mpv` binary) remain; dyld should ignore missing
  entries and use bundled libs — strip on macOS before notarization (`--warn-stale-rpath`).
- **yt-dlp / ffmpeg / deno** Intel sidecars are unchanged; confirm their Mach-O minOS ≤ 12.0 in
  release verify (`scripts/release/verify-macos.py`).

## If stolendata becomes unsuitable

Fallback research paths (not pursued if the pin works):

- Build from source via [mpv-build](https://github.com/mpv-player/mpv-build) with
  `MACOSX_DEPLOYMENT_TARGET=12`.
- Historical CI bundles discussed in [mpv#13529](https://github.com/mpv-player/mpv/pull/13529) — no
  pinned checksums located.

## Apple Silicon

Unchanged: official **mpv v0.41.0 macos-14-arm**, bundle minimum **14.0** (Sonoma).
