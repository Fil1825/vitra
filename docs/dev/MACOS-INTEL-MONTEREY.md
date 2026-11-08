# Intel macOS (Monterey+) and bundled mpv

## User-visible failure (v0.1.1)

On **Intel Macs running macOS 12 Monterey**, Vitra’s DMG showed macOS’s **prohibited** overlay on
`Vitra.app` when `LSMinimumSystemVersion` was **15.0** (Sequoia). That overlay is honest: the official
**mpv v0.41.0 macos-15-intel** sidecar’s main binary declares **minOS 15.0**, so the app could not run
on Monterey even if the plist were lowered.

## Bundled Intel mpv (from this branch)

| Item              | Value                                                                                                                                |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Source            | `scripts/release/build-intel-mpv.sh` and `scripts/release/intel-mpv-sources.json` (mpv 0.41.0, FFmpeg 7.1.1, deployment target 12.0) |
| Artifact          | `mpv-0.39.0.tar.gz`                                                                                                                  |
| SHA256            | `35ec81ad86a97b24956a8d0f4fa1ba2690b44ae7741c920e923620bcd7bd402a`                                                                   |
| mpv version       | **0.39.0** (Info.plist / `--version`)                                                                                                |
| Architecture      | thin **x86_64**                                                                                                                      |
| Mach-O minOS      | **11.0** on `mpv` and bundled dylibs (Monterey **12.7** ceiling passes `mach-minos.py`)                                              |
| Vitra Intel plist | **12.0** (`tauri.intel.conf.json`) — product floor for Monterey                                                                      |

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

## This candidate's Intel installer

Linux, Windows, and Apple Silicon installers from Actions run 42512576166
(commit `76ea04250e88d880c3b6a7e3abf182f29c6bbde2`) stay as built. Their artifact
zip digests are in `scripts/release/reused-artifacts.json`. Intel from that run
failed before the mpv link and is not pinned.

Pushing this pull request starts only the Intel package. `release.yml` listens
to `pull_request`, and the build job runs only when the matrix name is
`macOS-Intel` (`macos-15-intel`). Merging to `main` does not run `release.yml`.
A `v*` tag still builds all four platforms; that is not this candidate's path.
`workflow_dispatch` is not required. The job checks that application sources
still match `76ea042` before packaging. Playback on a Monterey machine is still
not verified, and this VM cannot sign the result.

## Source build

Intel mpv is produced by `scripts/release/build-intel-mpv.sh` from `intel-mpv-sources.json` on the macOS Intel release runner. The corresponding-source tarball includes that script, the pins, and `mach-minos.py` / `mach-deps.py`, so the offer can be rerun from its own directory. The script refuses a checksum mismatch, refuses a pkg-config view of host libraries, and refuses any Mach-O that needs newer than macOS 12.0. Homebrew is used only for compilers' companion tools (meson, nasm); shipped libraries are the pinned builds. HarfBuzz 10.1.0 is built with Meson.

mpv 0.41 enables `videotoolbox-pl` only when Vulkan and CoreVideo are both present. This recipe does not build a Vulkan loader, so `videotoolbox-pl` stays off. Hardware decode stays on `videotoolbox-gl` (`hwdec_vt.c`, `hwdec_mac_gl.c`) plus FFmpeg `--enable-videotoolbox` and `--enable-audiotoolbox`. `hwdec_mac_gl.c` calls CoreVideo and IOSurface, and mpv only adds the CoreVideo framework on the libplacebo path, so the recipe passes `-framework CoreVideo -framework IOSurface` on the link. The script also deletes mpv's Darwin `-L/usr/local/lib` link argument so Homebrew libraries are not on the link line.

## Apple Silicon

Unchanged: official **mpv v0.41.0 macos-14-arm**, bundle minimum **14.0** (Sonoma).
