#!/usr/bin/env bash
# Disposable local test only: no real app, user files, network, sudo or OS security changes.
set -euo pipefail
work="$(mktemp -d "${TMPDIR:-/tmp}/maintenance-fixture.XXXXXXXX")"
mounted=0
cleanup() {
  if [[ "$mounted" == 1 ]]; then
    if ! hdiutil detach "$work/mount" >/dev/null; then
      echo "Fixture mount cleanup failed: $work/mount" >&2
      return 1
    fi
  fi
  rm -rf -- "$work"
}
trap cleanup EXIT

make_app() {
  local app="$1" version="$2"
  mkdir -p "$app/Contents/MacOS"
  cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.example.maintenancefixture</string>
<key>CFBundleName</key><string>ExampleMaintenance</string>
<key>CFBundleExecutable</key><string>ExampleMaintenance</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
</dict></plist>
EOF
  cat > "$work/main.c" <<EOF
#include <stdio.h>
int main(void) { puts("$version"); return 0; }
EOF
  /usr/bin/clang -Wall -Wextra -Werror "$work/main.c" -o "$app/Contents/MacOS/ExampleMaintenance"
  /usr/bin/codesign --force --sign - --timestamp=none "$app"
  /usr/bin/codesign --verify --strict "$app"
}
check_app() {
  local app="$1" version="$2"
  /usr/bin/codesign --verify --strict "$app"
  [[ "$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$app/Contents/Info.plist")" == "$version" ]]
  [[ "$("$app/Contents/MacOS/ExampleMaintenance")" == "$version" ]]
}
mkdir -p "$work/installed" "$work/data" "$work/backup" "$work/stage" "$work/mount" "$work/recovery"
app="$work/installed/ExampleMaintenance.app"
make_app "$app" 1.0.0
printf '%s\n' '{"schema":1,"setting":"fixture-only"}' > "$work/data/settings.json"
check_app "$app" 1.0.0
ditto "$app" "$work/backup/ExampleMaintenance.app"
ditto "$work/data" "$work/backup/data"
check_app "$work/backup/ExampleMaintenance.app" 1.0.0
cmp "$work/data/settings.json" "$work/backup/data/settings.json"

make_app "$work/stage/ExampleMaintenance.app" 2.0.0
hdiutil create -srcfolder "$work/stage" -volname "Maintenance Fixture" -format UDZO "$work/fixture.dmg" >/dev/null
hdiutil verify "$work/fixture.dmg"
hdiutil attach -readonly -nobrowse -mountpoint "$work/mount" "$work/fixture.dmg" >/dev/null
mounted=1
check_app "$work/mount/ExampleMaintenance.app" 2.0.0
# Replace only the disposable test bundle after checking the source.
rm -rf -- "$app"
ditto "$work/mount/ExampleMaintenance.app" "$app"
check_app "$app" 2.0.0
cmp "$work/data/settings.json" "$work/backup/data/settings.json"
hdiutil detach "$work/mount" >/dev/null
mounted=0
# Recovery is verified in a separate directory, never by destroying current data.
printf '%s\n' '{"schema":2,"setting":"newer-fixture"}' > "$work/data/settings.json"
ditto "$work/backup/ExampleMaintenance.app" "$work/recovery/ExampleMaintenance.app"
ditto "$work/backup/data" "$work/recovery/data"
check_app "$work/recovery/ExampleMaintenance.app" 1.0.0
cmp "$work/recovery/data/settings.json" "$work/backup/data/settings.json"
grep -q '"schema":2' "$work/data/settings.json"
check_app "$app" 2.0.0
echo "PASS: fixture DMG verification, bundle replacement, preserved synthetic data, non-destructive backup recovery"
