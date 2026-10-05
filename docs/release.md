# Releasing Kept

Target: Windows 11 x64, per-user NSIS installer, no admin. One tag, one unsigned installer,
one manual signing step.

## What CI produces

- Every push: `ci.yml` runs `just check-core` on Linux and the full `just check` (including the
  Playwright critical path against the real app) on Windows, then uploads
  `kept-windows-unsigned-installer`.
- Every `v*` tag: `release.yml` runs `just check` and attaches the unsigned installer to a
  **draft, pre-release** GitHub release via `tauri-apps/tauri-action`.

## Cutting a release

1. `just check` on a Windows machine. Green means green; a Linux run ends with
   `E2E NOT RUN` and does not count (ADR-0009).
2. Bump `version` in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`
   (the same string in all three), add the release notes to `CHANGELOG.md`, commit
   (`chore(release): v0.x.y`).
3. `git tag v0.x.y && git push origin v0.x.y`.
4. Wait for `release.yml`; download the installer from the draft release.

## Signing (manual, deliberate)

Kept ships unsigned from CI because a code-signing certificate and its private key never belong
in a repository or a CI secret for a single-user app. Sign on the machine that holds the key:

```powershell
signtool sign /fd SHA256 /td SHA256 /tr http://timestamp.digicert.com `
  /f .\kept-codesign.pfx /p <pfx-password> `
  .\Kept_0.x.y_x64-setup.exe
signtool verify /pa /v .\Kept_0.x.y_x64-setup.exe
```

Then replace the asset on the draft release, un-draft it, and record the certificate thumbprint
in the release notes. Without this step Windows SmartScreen will warn on first run; that warning
is honest and expected for an unsigned build.

## Prerequisites on the user's machine

- Windows 11 x64.
- **WebView2 Runtime.** Kept does not bundle or download it (`webviewInstallMode: skip`). It is
  present on every up-to-date Windows 11; if missing, install the Evergreen runtime from
  Microsoft before running Kept.
- No admin rights: the installer runs per-user (`installMode: currentUser`).

## Building locally

```
pnpm install
just build            # unsigned NSIS installer under src-tauri/target/release/bundle/nsis/
just build-debug      # debug binary without an installer (what the E2E drives)
```

The Rust build compiles SQLCipher and OpenSSL from source (`bundled-sqlcipher-vendored-openssl`),
which needs Perl and takes several minutes the first time. On Windows that Perl must be
Strawberry Perl: `just` runs its recipes under Git Bash, whose own MSYS perl lacks the modules
OpenSSL's Configure loads, so set `OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe` (CI does)
before `just check` or `pnpm tauri build`.
