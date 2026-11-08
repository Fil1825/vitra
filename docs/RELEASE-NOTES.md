# Vitra 1.0.0-rc.1 — release candidate (not published)

**Status:** prepared in the `cursor/macos-metadata-intel-compat-668e` branch / PR [#22](https://github.com/Fil1825/vitra/pull/22). **Not** tagged, **not** uploaded to GitHub Releases, **not** marked Latest. Runtime acceptance on real Mac/Windows/Linux hardware is **still pending** (owner postponed acquaintance tests).

## Highlights since 0.1.1

- **Metadata:** Latin1-only ID3 CP1251 repair with Lofty normalization preserved; library rescan via `tag_decoder` v2.
- **Intel macOS:** `LSMinimumSystemVersion` **12.0**, pinned **stolendata mpv 0.39.0** (third-party, mpv.io-linked), offline Mach-O + `LC_LOAD_DYLIB` audits in CI tests.
- **Release hardening:** bounded HTTP retries for sidecar downloads; Intel mpv stale RPATH strip on macOS builders; [`THIRD-PARTY.md`](THIRD-PARTY.md) bundled; signing workflow inputs `sign_macos` / `sign_windows` documented in [`SIGNING-PREREQUISITES.md`](dev/SIGNING-PREREQUISITES.md).

## Platform requirements (documented, partially unverified)

| Target      | Minimum                          | Bundled playback                                   |
| ----------- | -------------------------------- | -------------------------------------------------- |
| macOS arm64 | 14 (Sonoma)                      | official mpv 0.41                                  |
| macOS Intel | 12 (Monterey)                    | stolendata mpv 0.39 — **no hardware sign-off yet** |
| Windows x64 | 10/11                            | mpv 0.41                                           |
| Linux deb   | compatible Debian / Ubuntu 24.04 | system `mpv`                                       |

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
