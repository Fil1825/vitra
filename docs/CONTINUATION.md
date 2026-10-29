# Stage 11 continuation

Last updated: 2026-10-29. This file is a durable handoff, not a release announcement.

## Owner decisions

- Remove Discord and Last.fm integrations from the release scope.
- Finish English/Russian, prepare installers/signing, documentation, and screenshots.
- The owner has **no paid Apple Developer account**. Prepare ad-hoc build candidates and
  the future Developer ID/notarization path; do not request credentials in chat.
- Preserve enough committed context to resume after usage limits. Continue independent work.

## Checkout

Work is on `codex/stage-11-release`, based on `claude/stage-10-my-wave` with the HTTPS fix
`5cee9b6`. Main still contains only the original specification/license: do not reset or base
this work on main. Use the existing checkout; do not create a worktree unless requested.
Stage 11 should be reviewed against the stage-10 branch. Do not tag or publish a release
until the owner chooses to do so.

## Implemented

- Removed integrations from the specification and updated the release plan/status.
- Audited all locale keys/interpolation and static JSX labels. Added missing Russian plural
  forms for download failures, localized startup failures and Diagnostics source labels.
- Added catalog/plural-rendering/static-label tests, including pre-initialization errors.
- Added Release candidates workflow: Apple Silicon/Intel macOS, Windows x64, Debian x64;
  verified helper fetches, complete bundles, nested macOS signing, actual packaged-helper
  checks, draft-only tag releases, optional future Developer ID/notarization secrets.
- Added Linux mpv package dependency and Intel macOS 15 minimum override.
- Added English/Russian READMEs, contribution/release/translation guides, release notes,
  issue/PR templates, and real browser screenshots (not native macOS captures).

## Validation already performed

- Frontend lint, formatting, type checking, tests, and production build passed (264 tests
  at the latest complete frontend run; rerun after later changes).
- Release helper and locale tests passed. Official actionlint 1.7.12 was downloaded with
  checksum verification and validated the release workflow. Python/plist parsing passed.
- A real optimized Debian bundle was built: Vitra 0.1.0, about 221 MiB. Its metadata contains
  mpv and GTK/WebKit dependencies. The extracted package's yt-dlp, FFmpeg, ffprobe, and Deno
  executed successfully through `verify-sidecars.mjs`.
- Browser UI switched English/Russian and Aurora Night/Light; no uncaught browser errors
  or horizontally overflowing Russian settings buttons were found. Screenshot provenance
  is in `docs/screenshots/README.md`.
- Native Linux GUI launch in this cloud sysroot hit WebKit's hardcoded system helper path.
  That is a local environment relocation limitation; do not claim native GUI validation.
- The full Rust suite hit the pre-existing lyrics backup test. The bug is reproducible:
  pruning frees a timestamp suffix, the next copy in the same millisecond reuses it, and
  pruning can immediately delete the newest backup. No assertion has been disabled.

## Remaining work

1. Fix the backup filename collision/pruning bug in a separate focused commit, with a
   deterministic same-millisecond regression test; it blocks a reliable green CI run.
2. Format the most recent signing-helper edits, rerun `npm run check:all`, actionlint, and
   relevant real-media/package checks. Inspect diffs and preserve all user changes.
3. Confirm the latest Debian bundle/sidecar verification after any packaging changes.
4. Push the stage-11 branch and open a PR against stage 10 if GitHub API access permits.
   Native Git read/push works via the platform proxy; GitHub REST/GraphQL API calls have
   returned Forbidden. Do not ask for a token merely because the API is blocked.
5. Read actual release CI results for macOS/Windows. Those native builds/signatures have
   not yet been verified here; official notarization requires the future account/cert.
6. Keep this file current with the final commit, test outcomes, and remaining limitations.

## Cloud commands

Initialize shells with `. /workspace/.vitra-env/activate.sh`, then work in `/workspace/vitra`.
This enables Rust, writable caches, and the GTK/WebKit development sysroot without sudo.

```sh
npm run check:all
/workspace/.vitra-env/actionlint/actionlint .github/workflows/release.yml
npm run bundle -- --config src-tauri/tauri.linux.conf.json --bundles deb
node scripts/release/verify-sidecars.mjs /workspace/.vitra-env/stage11-installed x86_64-unknown-linux-gnu
```

The extracted package directory above belongs to the existing validated bundle; re-extract
the current `.deb` there after rebuilding. Build outputs are under
`/workspace/.vitra-env/target` because activation sets an absolute Cargo target directory.

## Mac context

The owner created a local Codex project `vitra` and a task titled **Запусти тесты**. This
cloud thread can read that task but cannot send it commands or run Mac processes. Read
fresh results before making any native-Mac claims. The local task needs the stage-11 branch
for these new changes; merely rerunning tests in an old checkout does not validate them.
