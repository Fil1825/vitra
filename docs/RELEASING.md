# Preparing a Vitra release

Release preparation excludes Discord and Last.fm. The owner authorized promoting the completed stages to main and publishing downloadable
test installers. The first target version is 0.1.0, published as a prerelease.

## Packages

| Platform            | Package        | Minimum / engine                                                       |
| ------------------- | -------------- | ---------------------------------------------------------------------- |
| macOS Apple Silicon | App inside DMG | macOS 14+, included mpv                                                |
| macOS Intel         | App inside DMG | macOS 12+, stolendata mpv 0.39 (runtime not hardware-verified in-repo) |
| Windows x64         | NSIS installer | Windows 10/11, included mpv                                            |
| Linux x64           | Debian package | Ubuntu 24.04 / compatible Debian; system mpv dependency                |

FFmpeg, ffprobe, yt-dlp, and Deno are included. macOS and Windows include mpv and its
libraries as well. `npm run bundle` now rejects incomplete sidecar directories instead
of silently producing a package that relies on the developer's PATH. Linux packages
declare mpv as an installation dependency; an AppImage is not offered as a standalone build.

## Local build

```sh
npm ci
npm run fetch-sidecars -- --require-checksums
npm run check:all
npm run bundle -- --bundles app,dmg # macOS Apple Silicon
# Intel Mac: append --config src-tauri/tauri.intel.conf.json
# Windows: npm run bundle -- --bundles nsis
# Linux: npm run bundle -- --config src-tauri/tauri.linux.conf.json --bundles deb
```

`fetch-sidecars` refuses yt-dlp unless `SHA2-256SUMS.sig` is a valid signature from the
vendored release key. mpv, FFmpeg, and Deno publish checksums only. Use the correct host
architecture; see `scripts/sidecars/plan.mjs`. Cross type checks do
not produce or execute native installers. Keep `package.json`, the Cargo workspace version,
and the Tauri configuration version equal; the release version checker enforces this.

## Build automation

The **Release candidates** workflow builds all four targets. Branch builds and manual runs
save verified installers and each fetch manifest as downloadable artifacts for 14 days.
The manifest records helper versions, source URLs, and download checksums. Release fetches
require published checksums and retain TLS verification.

Every target runs `npm run check:all` on its native runner before packaging. A failing
frontend or Rust check prevents that platform's candidate from being uploaded.

The macOS job signs nested helper code before Tauri signs the enclosing app, then opens the
finished DMG and verifies its actual app, nested signatures, and executable helpers. The
Windows job silently installs the NSIS package into a temporary runner directory and runs
its included helpers. The Linux job inspects dependencies, extracts the Debian package,
and runs its included helpers. These checks do not replace a clean-machine playback test.

When ready, the owner can push a version tag matching the application version (for example
`v0.1.0`). Only after every platform build succeeds does the workflow collect the DMG/EXE/DEB files,
component manifests, and SHA-256 sums. It stages these in a draft, then publishes a
**prerelease**, without marking it as the latest stable release. A failed platform prevents
publication. Ordinary branch builds only upload Actions artifacts. Published assets are
never replaced by a rerun; use a new version/tag for a changed public release.

To publish without pushing a tag from a local Git client, manually run **Release candidates**
on `main` with **publish_release** enabled. It runs the same four native builds and package
checks, then creates `v<application version>` at that exact commit and publishes the prerelease.
This option is ignored on other branches. Existing public releases are never overwritten.

## Signing status and future setup

The owner currently has no Apple Developer account. macOS build candidates therefore use
ad-hoc signatures. This checks signature integrity, but it is **not** a Developer ID
signature or notarization and does not establish Gatekeeper trust. Windows builds are also
unsigned; a trusted Authenticode certificate is a separate future prerequisite.

When an Apple Developer account and Developer ID Application certificate are available,
add these **repository Actions secrets** in GitHub settings, never in source or chat:

| Secret                         | Purpose                                                        |
| ------------------------------ | -------------------------------------------------------------- |
| `APPLE_CERTIFICATE`            | Base64-encoded Developer ID Application `.p12` certificate     |
| `APPLE_CERTIFICATE_PASSWORD`   | Password protecting that certificate                           |
| `APPLE_SIGNING_IDENTITY`       | Full Developer ID Application identity                         |
| `APPLE_ID`                     | Apple account used for notarization                            |
| `APPLE_PASSWORD`               | App-specific notarization password                             |
| `APPLE_TEAM_ID`                | Developer team identifier                                      |
| `WINDOWS_CERTIFICATE`          | Base64 `.pfx` Authenticode certificate (optional Windows path) |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX password                                                   |

Choose **sign_macos** / **sign_windows** on a manual **Release candidates** run (both default
**false**). Missing credentials fail the job explicitly — no silent ad-hoc fallback when
trusted signing was requested. macOS: import cert, strip stale Intel mpv RPATHs, sign nested
helpers, Tauri notarization when enabled. Windows: import PFX, set `certificateThumbprint` via
transient config, `signtool verify` after build. This path cannot be validated here until the
owner configures secrets (see [SIGNING-PREREQUISITES.md](dev/SIGNING-PREREQUISITES.md)).

Before publishing signed packages, make a signed run of the exact version/tag and review
its verified assets. A default tag run remains an ad-hoc build until signed release runs
are explicitly enabled; do not describe those default artifacts as notarized.

## Final owner checks

Install on a clean Mac without Homebrew and on Windows without developer tools. Check local
playback, an HTTPS link, downloads, lyrics, My Wave, the mini player, both languages, and
restart/session restoration. Follow [the TLS regression instructions](TLS.md). Confirm that
the package uses its own helper programs in Settings → About → Diagnostics. Check system
media keys, tray/menu-bar controls, and the packaged app's theme files.

Review `RELEASE-NOTES.md`, screenshots, source links for the redistributed components, and
the optional author note/support link in `src/features/settings/about.ts`. Blank author and
support fields stay hidden. A hard crash on Windows can still orphan mpv; graceful shutdown
is supported, while a Job Object remains future work. Do not claim all native checks passed
based solely on a cloud build or a successful helper version command.
