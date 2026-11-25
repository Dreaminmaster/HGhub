# HGhub personal maintenance candidate v1.2.9

Owner elected manual installation rather than a self-updating executable. The app checks the fixed Dreaminmaster/HGhub GitHub Release list at startup or on request. Stable excludes prereleases; beta includes them. Only strictly versioned macOS Universal DMGs with exact repository URL, bounded size and valid GitHub SHA-256 metadata are eligible. Download rechecks selected version and digest, streams to a private temporary directory, verifies length and SHA-256, marks the file as downloaded (quarantine), then reveals it in Finder. Failed validation removes the temporary file. No automatic executable replacement, restart, remote code loading, elevated privileges, signing key, or security-policy bypass. User chooses when to install; do not disable Gatekeeper. SHA-256 against GitHub metadata checks integrity but cannot independently authenticate a compromised repository. Existing local data and WARP preferences remain outside the replaced bundle.

The v1.2.9 candidate workflow remains build-only, preserving the user-verified v1.2.8 baseline and existing distribution restrictions. A new candidate must pass CI and on-device testing before an explicitly approved distributable release is made available. There is currently no newer published macOS release to exercise download end-to-end. No scheduled jobs or automatic publication. Rollback, update history, upstream tracking and broader recovery are subsequent work.

## Maintenance increment
- Update HTTP destination allowlist is checked before every redirect hop. Metadata bodies are bounded (2 MiB and 20 seconds).
- Download progress is read-only and shared across settings mounts; no new permissions or self-install. Progress polling happens only while a user-initiated download is active.
- Explicit user-triggered component metadata checks compare the pinned code baselines to GitHub stable release tags; newer tags are review candidates, not proof of compatibility. No automatic source selection, code import or scheduled jobs.
- Copyable maintenance report contains version/channel/status and component metadata only. Existing network diagnostic remains separate.

## Local recovery archive
Completed verified downloads receive a bounded receipt in application data. History lists up to 20 receipts, not installed versions. Files remain in their original download folders; no installer is removed automatically. On a user request, the app checks that the recorded directory and DMG are ordinary non-symlink paths directly under Downloads, rechecks size and SHA-256, and reveals only that file in Finder. No install, privilege escalation, automatic rollback, download of historical packages, or modification of current app/data. Keep a copy of the current working application or installer separately before replacement; older app/data compatibility is not guaranteed. Existing pre-feature downloads are not silently imported.
## Update acceptance increment
- A newly detected app version is announced once per launch using a nonblocking toast. No automatic download, navigation, installation or network-mode changes.
- Update channel changes and repeated update checks are refused while an operation would become inconsistent.
- Dedicated browser fixture mounts the real update and archive UI, stubs only native commands, blocks external requests, and checks read-only detection, deduplication, user-initiated download, busy state, failure display, and archived installer visibility. Native download, Finder and OS authorization remain separate on-device acceptance steps.
- This follow-up branch intentionally leaves the running archive CI on PR #12 untouched. Merge into that branch only after its build has passed. No release or scheduled job is enabled.

## Isolated UI acceptance
The normal macOS workflow targets pull requests into main, which explains why PR #13 (targeting a development branch) has no run. A separate Ubuntu UI-only workflow checks the maintenance fixture on pushes to the acceptance branch. It uses mocked native calls, blocks external browser requests, does not run native packaging, import service adapters or publish artifacts. Download failure and retry are now covered explicitly.
