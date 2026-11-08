# Third-party components in Vitra packages

Vitra is licensed under [GPL-3.0-or-later](../LICENSE). Bundled executables are GPLv2+/GPLv3-compatible; this file lists what ships in installers and where to obtain corresponding source.

## Runtime sidecars (per platform manifest)

Each release asset `*.manifest.json` records downloaded file names, URLs, SHA-256 checksums, and (where applicable) provenance labels. Install **Settings → About → Diagnostics** shows which helpers the running app found.

| Component | Typical license | Source / builds |
|-----------|-----------------|-----------------|
| **mpv** (macOS/Windows in bundle; Linux system package) | GPLv2+ | Apple Silicon / Windows: [mpv-player releases](https://github.com/mpv-player/mpv/releases). **Intel macOS:** third-party [stolendata djinn build](https://laboratory.stolendata.net/~djinn/mpv_osx/) (mpv **0.39.0**, pinned SHA-256 in `scripts/sidecars/plan.mjs`). Corresponding source: [mpv-player/mpv](https://github.com/mpv-player/mpv) tag matching the bundled version; Intel binary is built by a third party — request rebuild recipe from the builder or compile from mpv source with deployment target ≤ 12. |
| **yt-dlp** | Unlicense / GPLv3+ (bundle) | [yt-dlp/yt-dlp](https://github.com/yt-dlp/yt-dlp) — release tarball verified with pinned OpenPGP key in `scripts/sidecars/`. |
| **FFmpeg / ffprobe** | LGPL-2.1+ / GPL-2+ (build-dependent) | macOS: [martin-riedl](https://ffmpeg.martin-riedl.de/); Win/Linux: [BtbN FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds). |
| **Deno** | MIT | [denoland/deno](https://github.com/denoland/deno) |
| **Tauri / WebView** | Various (see upstream) | [tauri-apps/tauri](https://github.com/tauri-apps/tauri) |

## Fonts

Outfit and Onest — SIL Open Font License 1.1 (see npm package metadata).

## Distribution obligation (GPL)

When you distribute Vitra binaries, you must offer the complete corresponding source for Vitra and comply with license terms for bundled GPL/LGPL components (offer of source, license text, and where required attribution). This repository is the Vitra source; manifests point at pinned helper URLs for reproducibility.
