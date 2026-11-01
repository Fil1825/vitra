# Contributing to Vitra

Vitra uses React, TypeScript, Vite, Tauri 2, and Rust. Read [the plan](docs/PLAN.md) and
[the specification](SPEC.md) before proposing a feature. Discord and Last.fm integrations
are outside the release scope.

## Environment

- **Node.js** 22.12 or newer (`engines.node` in `package.json`). npm 10 comes with Node 22.
- **Rust** 1.90 or newer (workspace `rust-version` in `Cargo.toml`, edition 2024). There is
  no `rust-toolchain` file. CI installs the latest stable toolchain, which is newer than 1.90.
- **Tauri CLI** 2.12, installed as an npm devDependency. Use `npm run tauri` (or `npx tauri`),
  and install the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for
  your platform.

On Linux, CI uses Ubuntu 24.04 and installs these packages before the Rust job:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev libgtk-3-dev libdbus-1-dev mpv
```

`npm run check:cross` also needs `llvm` (it provides `llvm-rc` for the Windows icon resource).
The real yt-dlp job installs `ffmpeg`. Linux has no official mpv binaries, so playback uses
the `mpv` package.

Keep the existing dependency lockfiles.

## Development

Use the existing checkout in isolated cloud tasks; create a worktree only when requested.
Do not put helper binaries, build outputs, credentials, or personal settings in Git.

```sh
npm ci
npm run fetch-sidecars -- --require-checksums
npm run tauri dev
```

`npm run fetch-sidecars` downloads mpv, yt-dlp, FFmpeg, ffprobe, and Deno for this machine
into `src-tauri/sidecars/<target>/`. yt-dlp's `SHA2-256SUMS` is accepted only when
`SHA2-256SUMS.sig` verifies under the release key in `scripts/sidecars/yt-dlp-public.key`.
A failed signature check stops the download. mpv, FFmpeg, and Deno do not publish an
OpenPGP signature of that kind. Options are listed by `npm run fetch-sidecars -- --help`.

## Checks

```sh
npm run lint           # eslint
npm run format:check   # prettier
npm run typecheck      # tsc -b
npm test               # vitest
npm run build          # production frontend build
npm run check          # all of the frontend checks above
npm run check:rust     # cargo fmt --check, clippy -D warnings, cargo test
npm run check:all      # frontend and Rust
npm run check:cross    # Linux only; clippy for macOS and Windows targets
```

The translation tests cover catalog keys, interpolation, plural forms, and literal JSX
labels. See [translations](docs/TRANSLATIONS.md).

Real media tests are opt-in. See [TLS tests](docs/TLS.md) and the examples in
[docs/dev/CLAUDE.md](docs/dev/CLAUDE.md).

```sh
VITRA_MPV_TESTS=1 cargo test -p mpv-ipc
VITRA_YTDLP=src-tauri/sidecars/<triple>/yt-dlp cargo test -p links --test real_ytdlp
```

State explicitly which real-media and native-platform checks ran; mocked unit tests do not
demonstrate actual playback or installation.

## Pull requests

Use a focused branch and Conventional Commits. Describe the problem and final behavior,
list meaningful validation, and include a **How to verify on macOS** section with commands
and expected behavior. Call out native-platform checks that need the owner's Mac or Windows
machine. Keep pure logic in the workspace crates; use the typed frontend IPC wrappers.

For releases and signatures, follow [RELEASING.md](docs/RELEASING.md). Tagging prepares a
draft for review; publication remains a separate owner action.
