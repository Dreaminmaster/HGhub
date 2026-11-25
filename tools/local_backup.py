#!/usr/bin/env python3
"""Explicit macOS local backup. No network, installation, or automatic restore."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile


def resolve_selected(raw):
    path = Path(raw).expanduser().absolute()
    # Resolve symlinks only in ancestors (macOS /var, for example); refuse a selected symlink.
    if path.is_symlink() or not path.is_dir():
        raise ValueError("Select an existing ordinary directory.")
    return path.resolve(strict=True)


def overlaps(a, b):
    return a == b or a in b.parents or b in a.parents


def manifest(root):
    result = {}
    def fail_walk(error):
        raise error
    for folder, dirs, files in os.walk(root, topdown=True, followlinks=False, onerror=fail_walk):
        for name in sorted(dirs + files):
            p = Path(folder) / name
            rel = p.relative_to(root).as_posix()
            mode = p.lstat().st_mode
            if stat.S_ISLNK(mode):
                target = os.readlink(p)
                if os.path.isabs(target):
                    raise ValueError("Absolute symlink in selected source: " + rel)
                resolved = p.resolve(strict=True)
                if resolved != root and root not in resolved.parents:
                    raise ValueError("External symlink in selected source: " + rel)
                result[rel] = ["link", target]
                if name in dirs:
                    dirs.remove(name)
            elif stat.S_ISDIR(mode):
                result[rel] = ["dir"]
            elif stat.S_ISREG(mode):
                if p.lstat().st_nlink != 1:
                    raise ValueError("Hard-linked file in selected source: " + rel)
                h = hashlib.sha256()
                with p.open("rb") as f:
                    for chunk in iter(lambda: f.read(1024 * 1024), b""):
                        h.update(chunk)
                result[rel] = ["file", p.stat().st_size, h.hexdigest()]
            else:
                raise ValueError("Special file in selected source: " + rel)
    return result


def backup(app, data, parent):
    if sys.platform != "darwin":
        raise ValueError("This utility uses macOS ditto.")
    app, data, parent = map(resolve_selected, (app, data, parent))
    if app.suffix != ".app":
        raise ValueError("Select an application bundle ending in .app.")
    if any(overlaps(a, b) for a, b in ((app, data), (app, parent), (data, parent))):
        raise ValueError("Sources and backup destination must be separate.")
    mode = parent.stat()
    if mode.st_uid != os.getuid() or mode.st_mode & 0o077:
        raise ValueError("Choose a private backup parent owned by you (mode 0700).")
    before = {"application": manifest(app), "data": manifest(data)}
    output = Path(tempfile.mkdtemp(prefix="local-backup-", dir=parent))
    try:
        for name, source in (("application", app), ("data", data)):
            dest = output / name
            subprocess.run(["/usr/bin/ditto", str(source), str(dest)], check=True)
            if manifest(dest) != before[name] or manifest(source) != before[name]:
                raise ValueError("Source changed or copy verification failed: " + name)
        if manifest(app) != before["application"] or manifest(data) != before["data"]:
            raise ValueError("Source changed during backup.")
        # The final receipt name is visible only after a complete, flushed write.
        pending = output / ".receipt-incomplete.json"
        with pending.open("x", encoding="utf-8") as f:
            os.fchmod(f.fileno(), 0o600)
            json.dump({"schema": 1, "app_name": app.name, "manifest": before}, f, ensure_ascii=False)
            f.flush()
            os.fsync(f.fileno())
        pending.rename(output / "verified-receipt.json")
        return output
    except Exception:
        # Preserve a failed copy for the user's review; no automatic recursive deletion.
        print("Incomplete backup, not verified: " + str(output), file=sys.stderr)
        raise


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--app", required=True)
    p.add_argument("--data", required=True)
    p.add_argument("--destination", required=True)
    p.add_argument("--confirm-closed", action="store_true", help="Confirm the selected app is closed.")
    args = p.parse_args()
    if not args.confirm_closed:
        p.error("Quit the application before backing up; confirm with --confirm-closed.")
    try:
        print("Verified local backup: " + str(backup(args.app, args.data, args.destination)))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as exc:
        p.exit(1, str(exc) + "\n")


if __name__ == "__main__":
    main()
