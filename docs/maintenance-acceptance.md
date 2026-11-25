# Maintenance acceptance record — v1.2.9 candidate

## Verified evidence

- Native macOS candidate at 591b6a12de43d2de1d3310cf95615ecf17f48940: Actions run 45618684306 succeeded on Universal and x64. Rust: 59 passed, 0 failed, 6 ignored; DMG integrity verified. No Release published.
- Maintenance UI at 412338af659054f7d38d37e644be7f21b4452e74: Actions run 45621178932 succeeded (TypeScript and browser fixture). Native commands are mocked; this is not a native installation test.
- PR 12 and PR 13 remain separate draft branches. No integrated native build of the latest UI head has been verified.

## Local backup before any replacement

Use the operating system's normal backup facility (e.g. Time Machine) or keep a local copy of the current working application and data with restrictive access. Quit the application before copying data. Verify that the backup exists and can be read before replacing the application. Do not post backup files, session information, proxy credentials or device private keys in GitHub or diagnostic reports. The portable preferences backup includes only the update channel and startup-check preference; it is not a complete data backup. The installer history records downloads, not installed or working versions. Retain a known-good backup separately.

## Remaining acceptance gates

- [ ] Verify integrated maintenance UI and native code with a local fixture or an authorized-source build, without importing restricted service adapters.
- [ ] Verify update metadata handling for missing releases, HTTP errors, timeouts and invalid asset hashes.
- [ ] Verify a complete user-approved download and manual installation using an authorized test package on Apple Silicon.
- [ ] Confirm that preferences and local data survive a test update, and that a known-good local backup can be restored without overwriting newer incompatible data.
- [ ] Review third-party licenses, distribution permissions and service authorization before any distribution. Personal noncommercial use does not itself establish distribution rights.
- [ ] Only publish an explicitly authorized package after these checks. Keep existing releases unchanged until then.

No scheduled jobs, automatic code imports, new publication, security-policy exceptions, or automatic replacement are authorized by this record.

## Boundary regression follow-up
The UI now rejects malformed or unknown update metadata, clears stale successful check times on new checks, and prevents a download during a check. The fixture covers no eligible release, current version, invalid digest, unknown status and persisted preference reload. These changes are limited to the local UI acceptance branch. Native HTTP timeouts, native byte-stream integrity, actual macOS installation and release authorization remain separate gates. Latest passing evidence before this follow-up: run 45621484879.
