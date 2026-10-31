# Stage 11 continuation

Last updated: 2026-10-31. This file is a durable handoff, not a release announcement.

## Owner decisions

- Remove Discord and Last.fm integrations from the release scope.
- Finish English/Russian, prepare installers/signing, documentation, and screenshots.
- The owner has **no paid Apple Developer account**. Prepare ad-hoc build candidates and
  the future Developer ID/notarization path; do not request credentials in chat.
- Preserve enough committed context to resume after usage limits. Continue independent work.

## Checkout and publication authorization

The owner has now requested promoting the application to `main` and publishing downloadable
installers. This supersedes the earlier draft-only/no-tag plan. Use a GitHub prerelease for
0.1.0 because official signing and clean-machine acceptance remain pending.

`main` is an ancestor of `codex/stage-11-release`; promotion can preserve all stage history
with a normal fast-forward push. Do not force-push or reset unrelated changes. The release
workflow publishes only when all native platform checks and packaged-helper checks pass.

Saved implementation commits include `5bad5c7` (stage 11), `21af70d` (backup retention),
`a7b940f` (local-analysis wording), and `27cab47` (native checks and previous handoff).

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

## Publication progress and remaining work

- Main is now promoted through `f684e2c`; no tag or public release exists yet as of the
  2026-10-31 continuation. Run `41127166578` passed Linux but failed Windows's two
  Unix-specific import test fixtures and macOS notarization with empty Apple credentials.
- Commit `e4a2237` makes those fixtures native to the test host and unsets notarization
  credentials for ad-hoc builds. Local import tests pass (33 tests). The workflow now
  also supports a manual `publish_release` run on `main`, preserving all native/package
  checks and the refusal to replace an existing public release. Wait for the actual
  publishing job and assets before claiming completion.
- Manual publishing run `41184176712` passed Linux and built a valid Apple Silicon DMG.
  Windows then exposed more Unix-specific folder fixtures and mixed-separator lookup
  expectations; macOS verification stopped at the image's own interactive GPL notice.
  The next continuation fixes these fixtures, feeds that notice's response during the
  read-only DMG check, collects all Rust test failures in one run, and caches failed native
  builds to avoid repeating dependency compilation. All 96 library tests pass locally.
- GitHub API calls return Forbidden in this cloud. Public GitHub HTML pages are readable
  with curl, including Actions run summaries and failure annotations; full logs require
  sign-in. Do not mistake the API restriction for missing Git push access.
- Run `40945353729` produced Linux and Windows artifacts but failed at nested macOS signing.
  Run `41125421524` failed Windows quality checks. These are not complete release results.
- Added LF checkout rules to avoid Windows checkout changing source line endings, and
  failure annotations so subsequent native failures can be diagnosed from the public page.
- Finish checks, push the main promotion and version tag, then inspect the resulting native
  jobs. Fix failures before describing any installer as published. Never move a public tag.
- Verify clean installation and playback on all supported systems and official signing
  when accounts/certificates are available. Keep this handoff current with actual results.

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
