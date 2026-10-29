# Contributing to Vitra

Vitra uses React, TypeScript, Vite, Tauri 2, and Rust. Read [the plan](docs/PLAN.md) and
[the specification](SPEC.md) before proposing a feature. Discord and Last.fm integrations
are outside the release scope.

## Development

Install stable Rust, Node.js 22.12 or newer, and the
[Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.
On Linux also install mpv. Keep the existing dependency lockfiles.

```sh
npm ci
npm run fetch-sidecars -- --require-checksums
npm run tauri dev
```

Use the existing checkout in isolated cloud tasks; create a worktree only when requested.
Do not put helper binaries, build outputs, credentials, or personal settings in Git.

## Checks

```sh
npm run check:all
npm run check:cross # Linux only; requires LLVM and the cross targets
```

The first command runs ESLint, Prettier, TypeScript, frontend tests, the frontend build,
Rust formatting, Clippy, and workspace tests. The translation tests cover catalog keys,
interpolation, plural forms, and literal JSX labels. See [translations](docs/TRANSLATIONS.md).

Real media tests are opt-in. See [TLS tests](docs/TLS.md) and the examples in `CLAUDE.md`.
State explicitly which real-media and native-platform checks ran; mocked unit tests do not
demonstrate actual playback or installation.

## Pull requests

Use a focused branch and Conventional Commits. Describe the problem and final behavior,
list meaningful validation, and include a **How to verify on macOS** section with commands
and expected behavior. Call out native-platform checks that need the owner's Mac or Windows
machine. Keep pure logic in the workspace crates; use the typed frontend IPC wrappers.

For releases and signatures, follow [RELEASING.md](docs/RELEASING.md). Tagging prepares a
draft for review; publication remains a separate owner action.
