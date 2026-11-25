# Standalone maintenance lab

This branch's working tree contains only the reusable update integrity and local receipt code and inert test fixtures. It does not contain the existing application's source, third-party compatibility overlay, network routing, player, installation or packaging scripts. The original application branches are unchanged; this branch is not intended for merging into main.

The core was extracted from maintenance commit 876717f4f5bee1c8f6844d94b6d9ae045b3107bd, with generic example file names. Previous independent tests passed (run 45644873036). The new lab additionally tests real loopback HTTP responses using a local server: exact bytes, truncated/corrupt responses, HTTP 503, denied redirects, and timeout. Test bytes are not executables. The transport is a test harness, not the application's production HTTP implementation. No requests to external content providers, app packaging, release publishing, or scheduled jobs.

Run cargo test. CI tests on Linux and macOS; successful macOS CI is not an Apple Silicon desktop installation test. User-approved manual installation of an authorized package and full app/data restoration remain unverified. No update signing private key required by this lab.
