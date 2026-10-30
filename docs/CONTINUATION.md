# Stage 11 continuation

Last updated: 2026-10-30. This file is a durable handoff, not a release announcement.

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

Saved implementation commits: `5bad5c7` (release preparation and translations), `21af70d`
(lyrics backup fix), and `a7b940f` (local-analysis wording). Later commits update this handoff
and the workflow trigger. Use the latest remote branch head rather than checking out one
of these intermediate commits.

## Implemented

- Removed integrations from the specification and updated the release plan/status.
- Audited all locale keys/interpolation and static JSX labels. Added missing Russian plural
  forms for download failures, localized startup failures and Diagnostics source labels.
- Clarified the completed-analysis label in both languages: it covers local files, not the
  separately counted links that may still be queued during a pause between requests.
- Added catalog/plural-rendering/static-label tests, including pre-initialization errors.
- Added Release candidates workflow: Apple Silicon/Intel macOS, Windows x64, Debian x64;
  verified helper fetches, complete bundles, nested macOS signing, actual packaged-helper
  checks, draft-only tag releases, optional future Developer ID/notarization secrets.
- Added Linux mpv package dependency and Intel macOS 15 minimum override.
- Added English/Russian READMEs, contribution/release/translation guides, release notes,
  issue/PR templates, and real browser screenshots (not native macOS captures).
- Fixed lyrics backup pruning: multiple edits in one millisecond no longer reuse a pruned
  suffix and immediately delete the newest copy. A deterministic regression test covers it.
- Release candidates rebuild for every push to the stage-11 branch, including app fixes.
  Each native runner must pass `npm run check:all` before packaging its installer.

## Validation already performed

- `npm run check:all` passed: frontend lint/formatting/type checking/production build,
  264 frontend tests, Rust formatting/clippy, and 452 Rust tests. Two real-network tests
  remain opt-in in the ordinary suite; no failing assertion was disabled.
- `npm run check:cross` passed for Apple Silicon macOS and Windows x64. These checks compile
  Rust types without linking or executing native applications.
- The real FFmpeg sidecar passed local trusted HTTPS, rejection of an untrusted certificate,
  and plain HTTP through `scripts/test-ffmpeg-tls.py`.
- Release helper and locale tests passed. Official actionlint 1.7.12 was downloaded with
  checksum verification and validated the release workflow. Python/plist parsing passed.
- A real optimized Debian bundle was built: Vitra 0.1.0, about 221 MiB. Its metadata contains
  mpv and GTK/WebKit dependencies. The extracted package's yt-dlp, FFmpeg, ffprobe, and Deno
  executed successfully through `verify-sidecars.mjs`. The final rebuild includes `a7b940f`
  and the backup fix. The installed executable matches the build except for Tauri's three-byte
  bundle-type patch (`UNK` → `DEB`), with the same ELF build ID.
  Package SHA-256: `718412db6d264498376a762bbeae5bc19c09c503fe79d5874de3f442ef42f297`.
- Browser UI switched English/Russian and Aurora Night/Light; no uncaught browser errors
  or horizontally overflowing Russian settings buttons were found. Screenshot provenance
  is in `docs/screenshots/README.md`.
- Native Linux GUI launch in this cloud sysroot hit WebKit's hardcoded system helper path.
  That is a local environment relocation limitation; do not claim native GUI validation.
- The pre-existing lyrics backup failure is fixed in `21af70d`; both backup tests and the
  full Rust suite now pass.

## Remaining work

1. Inspect the working tree and preserve user edits before continuing. Fetch the latest
   `codex/stage-11-release`; do not replace it with main or stage 10.
2. Open a PR against stage 10 when API access is available. Creation with `gh pr create`
   and listing Actions runs both returned Forbidden on 2026-10-30. Native Git read/push
   works via the platform proxy. Do not ask for a token merely because the API is blocked.
   [Review the branch comparison](https://github.com/Fil1825/vitra/compare/claude%2Fstage-10-my-wave...codex%2Fstage-11-release?expand=1).
3. Read actual release CI results for macOS/Windows. Those native builds/signatures have
   not yet been verified here; official notarization requires the future account/cert.
4. Run clean-machine acceptance on macOS and Windows using `docs/RELEASING.md` and
   `docs/TLS.md`; replace browser screenshots with native captures when available.
5. Keep this file current with the final commit, test outcomes, and remaining limitations.

## Cloud commands

Initialize shells with `. /workspace/.vitra-env/activate.sh`, then work in `/workspace/vitra`.
This enables Rust, writable caches, and the GTK/WebKit development sysroot without sudo.

```sh
npm run check:all
/workspace/.vitra-env/actionlint/actionlint .github/workflows/release.yml
npm run bundle -- --config src-tauri/tauri.linux.conf.json --bundles deb
dpkg-deb -x /workspace/.vitra-env/target/release/bundle/deb/Vitra_0.1.0_amd64.deb /workspace/.vitra-env/stage11-installed-final
node scripts/release/verify-sidecars.mjs /workspace/.vitra-env/stage11-installed-final x86_64-unknown-linux-gnu
```

The extracted package directory above belongs to the existing validated bundle; re-extract
the current `.deb` there after rebuilding. Build outputs are under
`/workspace/.vitra-env/target` because activation sets an absolute Cargo target directory.

## Mac context

The owner created a local Codex project `vitra` and a task titled **Запусти тесты**. This
cloud thread can read that task but cannot send it commands or run Mac processes. Read
fresh results before making any native-Mac claims. The local task needs the stage-11 branch
for these new changes; merely rerunning tests in an old checkout does not validate them.

The latest local task history was read on 2026-10-30: stage 10 was updated to `5cee9b6`,
tested, and launched. It reports 254 frontend / 451 Rust tests and real HTTPS/media checks
passing. Its actual SQLite read shows three analyzed streams, two successful and one
unavailable YouTube video; the owner then saw the processed counter progress to five.
The original certificate failure did not reproduce on those checked streams. Two running
app copies and a paused analyzer initially confused the check; one current copy was left
processing. This is evidence for stage 10, not stage-11 DMG/signing acceptance.
