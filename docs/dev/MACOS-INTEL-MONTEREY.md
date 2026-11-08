# Intel Macs on macOS Monterey (12.x)

## User report

A MacBook Pro (Retina, 13-inch, Early 2015) on **macOS Monterey 12.7.6** saw Vitra's Intel `.dmg` show the
"prohibited" overlay on `Vitra.app` when `LSMinimumSystemVersion` was **15.0** (Sequoia). That overlay is
Finder comparing the bundle minimum to the running system, not Gatekeeper.

## Investigation (v0.1.1 / mpv v0.41.0)

| Component                                  | Intel x86_64                     | Mach-O minimum (measured)       |
| ------------------------------------------ | -------------------------------- | ------------------------------- |
| Vitra (Tauri)                              | ships as universal? x86_64 slice | set via `tauri.intel.conf.json` |
| **mpv** (`mpv-v0.41.0-macos-15-intel.zip`) | main `mpv` binary                | **15.0.0**                      |
| mpv bundled dylibs (sample of 112)         | mostly                           | **14.0.0**, some **15.0.0**     |
| FFmpeg (Martin Riedl amd64)                | static                           | **12.0.0**                      |
| yt-dlp folder build                        | Python + `_internal`             | not fully audited here          |

Official [mpv-player releases](https://github.com/mpv-player/mpv/releases) list **no** Intel macOS asset older
than `macos-15-intel` for v0.41.x / v0.42.x. Historical tags (v0.32–v0.37) publish **no** macOS zips on
GitHub releases.

`scripts/release/mach-minos.py` fails a Monterey ceiling (`--max 12.7`) against the fetched Intel mpv bundle
(dozens of dylibs ≥ 14.0, main binary 15.0).

## Conclusion

A **self-contained** Vitra Intel package that **plays audio on Monterey 12.7.6** is **not achievable** with
upstream mpv sidecars pinned today. Lowering `LSMinimumSystemVersion` alone would only remove the DMG overlay
while bundled mpv still could not start.

Reasonable paths (not implemented in v0.1.1):

1. **Reproducible from-source mpv** for x86_64 with `MACOSX_DEPLOYMENT_TARGET=12` and matching FFmpeg/libs
   (hours of macOS CI, cache, codesign of ~100 dylibs).
2. **Wait for upstream** Intel builds with ≤ 12 deployment (none offered now).
3. **Split SKUs**: Intel build requires **macOS 15 (Sequoia)** — maximum OS on many pre-2018 Macs is Monterey,
   so those machines remain unsupported until (1) or (2).

Vitra's Intel plist minimum is kept at **15.0** to match bundled mpv. Apple Silicon stays at **14.0** (Sonoma)
with `macos-14-arm` mpv.

Release CI runs `mach-minos.py --max <plist minimum>` on the app binary and bundled `sidecars/` (see
`scripts/release/verify-macos.py`).
