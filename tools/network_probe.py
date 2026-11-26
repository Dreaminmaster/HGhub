#!/usr/bin/env python3
"""Single-request diagnostics for an authorized HTTP(S) health endpoint."""
import argparse
import json
import subprocess
import time
from urllib.parse import urlsplit


def classify(code, status):
    if code:
        return {5:"proxy_dns", 6:"host_dns", 7:"connection_failed",
                28:"timeout", 35:"tls_handshake", 51:"certificate_rejected",
                60:"certificate_rejected", 77:"local_certificate_configuration",
                52:"empty_response", 55:"send_failed", 56:"receive_failed",
                97:"proxy_handshake"}.get(code, "transport_error")
    if 200 <= status <= 299:
        return "http_reachable"
    if 300 <= status <= 399:
        return "redirect_not_followed"
    if status in (401,403,407,451):
        return "access_denied"
    if status == 429:
        return "rate_limited"
    if status in (405,501):
        return "head_not_supported"
    if 400 <= status <= 499:
        return "client_error"
    if 500 <= status <= 599:
        return "server_error"
    return "unexpected_response"


def validate(url):
    parts = urlsplit(url)
    if (parts.scheme not in ("http","https") or not parts.hostname
            or parts.username is not None or parts.password is not None
            or parts.query or parts.fragment
            or any(ord(c) <= 32 or ord(c) == 127 for c in url)
            or len(url) > 2048):
        raise ValueError("Use an authorized HTTP(S) health URL without credentials, query or fragment.")
    # Access port to reject malformed or out-of-range port values.
    _ = parts.port


def probe(url):
    validate(url)
    start = time.monotonic()
    try:
        result = subprocess.run(
            ["curl", "--disable", "--silent", "--head", "--connect-timeout", "4",
             "--max-time", "8", "--proto", "=http,https", "--globoff",
             "--output", "/dev/null", "--write-out", "%{http_code}", "--url", url],
            capture_output=True, text=True, timeout=10, check=False)
        raw = result.stdout.strip()
        status = int(raw) if len(raw) == 3 and raw.isascii() and raw.isdigit() else 0
        category = classify(result.returncode, status)
    except subprocess.TimeoutExpired:
        status, category = 0, "timeout"
    except OSError:
        status, category = 0, "diagnostic_unavailable"
    return {"category":category, "http_status":status or None,
            "elapsed_ms":round((time.monotonic()-start)*1000)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("url", help="Health endpoint you are authorized to test")
    args = p.parse_args()
    try:
        report = probe(args.url)
    except ValueError as exc:
        p.error(str(exc))
    print(json.dumps(report, ensure_ascii=False))
    return 0 if report["category"] == "http_reachable" else 1


if __name__ == "__main__":
    raise SystemExit(main())
