# Standalone maintenance lab

This branch's working tree contains only the reusable update integrity and local receipt code and inert test fixtures. It does not contain the existing application's source, third-party compatibility overlay, network routing, player, installation or packaging scripts. The original application branches are unchanged; this branch is not intended for merging into main.

The core was extracted from maintenance commit 876717f4f5bee1c8f6844d94b6d9ae045b3107bd, with generic example file names. Previous independent tests passed (run 45644873036). The new lab additionally tests real loopback HTTP responses using a local server: exact bytes, truncated/corrupt responses, HTTP 503, denied redirects, and timeout. Test bytes are not executables. The transport is a test harness, not the application's production HTTP implementation. No requests to external content providers, app packaging, release publishing, or scheduled jobs.

Run cargo test. CI tests on Linux and macOS; successful macOS CI is not an Apple Silicon desktop installation test. User-approved manual installation of an authorized package and full app/data restoration remain unverified. No update signing private key required by this lab.

## Runner status
Run 45645481470: Linux 16 tests passed, zero failed or skipped. The macos-latest job was cancelled before any recorded steps; no log is available. The exact cancellation cause is unconfirmed. Switch to macos-15-intel, which previously completed the repository's native tests, and cancel superseded lab runs. This is an Intel macOS test, not an M5 installation test. No packaging or publication.

## macOS package lifecycle fixture
Run 45691195186 passed 16 tests on each of Linux and Intel macOS. A separate macOS-only test now creates two minimal, locally compiled ExampleMaintenance.app bundles and a DMG inside a temporary directory. It verifies the disk image, mounts it read-only, verifies the copied app, checks that synthetic app data survives replacement, and verifies a known-good backup in a separate recovery directory while retaining the newer state. It never touches /Applications, real user data, system security settings, content services or the original app. The locally compiled fixture is ad-hoc signed; this is not a Gatekeeper, notarization, remote-download or M5 desktop acceptance test. No artifact upload or release. Latest fixture result pending.
