# Signing prerequisites (owner / parent)

This file is for humans configuring GitHub Actions. **Never commit certificates, passwords, or tokens.**

## What this cloud agent could verify (2026-11-08)

| Check | Result |
|-------|--------|
| `gh secret list` on `Fil1825/vitra` | **HTTP 403** — integration cannot read secret **names or values** |
| Repository GitHub Environments | **none** (`total_count: 0`) |
| Workflow wiring for trusted macOS | **present** — `sign_macos` input, see below |
| Workflow wiring for trusted Windows | **added with `sign_windows`** — requires new secrets (see below) |

Absence of API access does **not** prove secrets are missing on GitHub; the owner must confirm in **Settings → Secrets and variables → Actions**.

## macOS (Developer ID + notarization)

Required when running **Release candidates** with **`sign_macos: true`** (default is **false** → ad-hoc nested helpers + unsigned app envelope per current workflow).

| GitHub Actions secret | Purpose |
|----------------------|---------|
| `APPLE_CERTIFICATE` | Base64 `.p12` (Developer ID Application) |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` password |
| `APPLE_SIGNING_IDENTITY` | Full identity string (e.g. `Developer ID Application: …`) |
| `APPLE_ID` | Apple ID for notarization API |
| `APPLE_PASSWORD` | App-specific password |
| `APPLE_TEAM_ID` | Team ID |

Pipeline steps: import cert → `scripts/release/sign-macos.mjs` on sidecars → optional `strip-macos-rpaths.mjs` on Intel mpv → `tauri build` with notarization env → `verify-macos.py --signed` (stapler + `spctl`).

## Windows (Authenticode)

Required when **`sign_windows: true`** (default **false** → unsigned NSIS).

| GitHub Actions secret | Purpose |
|----------------------|---------|
| `WINDOWS_CERTIFICATE` | Base64 `.pfx` code-signing cert |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX password |

Build imports the PFX into the ephemeral runner store, sets `TAURI_WINDOWS_SIGN_COMMAND` for the NSIS bundle step, and verifies with `signtool verify`. SmartScreen reputation is **not** instant even with a valid signature.

## Secure handoff

Use Cursor/GitHub secret forms or Actions settings — not chat. After secrets exist, run **Release candidates** manually on `main` (or a release branch) with `publish_release: false`, `sign_macos: true` / `sign_windows: true`, and inspect **14-day artifacts** only (no public release until owner approves).
