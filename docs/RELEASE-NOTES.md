# Vitra 1.0.0-rc.1 — release candidate (not published)

**Status:** prepared in the `cursor/macos-metadata-intel-compat-668e` branch / PR [#22](https://github.com/Fil1825/vitra/pull/22). **Not** tagged, **not** uploaded to GitHub Releases, **not** marked Latest. Runtime acceptance on real Mac/Windows/Linux hardware is **still pending** (owner postponed acquaintance tests).

**Signing:** default builds stay ad-hoc / unsigned (`sign_macos` / `sign_windows` false). Trusted signing stays fail-closed without credentials.

**Intel mpv:** built from pinned sources in [`scripts/release/intel-mpv-sources.json`](scripts/release/intel-mpv-sources.json) for macOS 12, not from the stolendata binary. The corresponding-source tarball is produced next to the sidecar. Mach-O checks are not a Monterey playback test; that hardware run is still postponed.

## Highlights since 0.1.1

- **Metadata:** Latin1-only ID3 CP1251 repair with Lofty normalization preserved; library rescan via `tag_decoder` v2.
- **Intel macOS:** `LSMinimumSystemVersion` **12.0**, mpv 0.41.0 built from pinned sources (FFmpeg 7.1.1, libplacebo 7.349.0, libass 0.17.3) with deployment target 12.
- **Release hardening:** bounded HTTP retries for sidecar downloads; Intel mpv stale RPATH strip on macOS builders; [`THIRD-PARTY.md`](THIRD-PARTY.md) bundled; signing workflow inputs `sign_macos` / `sign_windows` documented in [`SIGNING-PREREQUISITES.md`](dev/SIGNING-PREREQUISITES.md).

## Platform requirements (documented, partially unverified)

| Target      | Minimum                          | Bundled playback                                     |
| ----------- | -------------------------------- | ---------------------------------------------------- |
| macOS arm64 | 14 (Sonoma)                      | official mpv 0.41                                    |
| macOS Intel | 12 (Monterey)                    | source-built mpv 0.41 — **no hardware sign-off yet** |
| Windows x64 | 10/11                            | mpv 0.41                                             |
| Linux deb   | compatible Debian / Ubuntu 24.04 | system `mpv`                                         |

## Signing (default vs trusted)

| Build                                        | macOS                                                | Windows                                     |
| -------------------------------------------- | ---------------------------------------------------- | ------------------------------------------- |
| Default workflow (`sign_*` false)            | Ad-hoc nested helpers, no notarization               | Unsigned NSIS                               |
| Trusted (`sign_macos` / `sign_windows` true) | Developer ID + notarization when Apple secrets exist | Authenticode when `WINDOWS_*` secrets exist |

SmartScreen and Gatekeeper friction may remain even with valid signatures until reputation/notarization propagate.

## Clean-machine checklist

See [RELEASING.md](RELEASING.md) § Final owner checks — **not completed** for this RC in project records.

## Prior 0.1.1 notes

0.1.1 remains the latest **published** GitHub prerelease (`v0.1.1`) built from `main` with Intel **15+** / official mpv; do not confuse with this RC.
