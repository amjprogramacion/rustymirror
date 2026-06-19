#!/usr/bin/env python3
"""Assemble a complete updater latest.json from the assets already uploaded to a
GitHub release, covering every platform built by the matrix.

Runs once after all matrix legs finish (see the `finalize-updater` job), so it is
free of the race condition you get when each parallel job writes its own
single-platform latest.json and they clobber each other.

Required env vars: GITHUB_TOKEN, TAG, REPO
"""
import json
import os
import sys
import urllib.request
import urllib.error
from datetime import datetime, timezone

TOKEN = os.environ["GITHUB_TOKEN"]
TAG = os.environ["TAG"]
REPO = os.environ["REPO"]
VERSION = TAG[1:] if TAG.startswith("v") else TAG
API = f"https://api.github.com/repos/{REPO}"


def api(url, method="GET", data=None, headers=None, raw=False):
    hdrs = {"Authorization": f"token {TOKEN}", "User-Agent": "rustymirror-ci"}
    if headers:
        hdrs.update(headers)
    req = urllib.request.Request(url, data=data, method=method, headers=hdrs)
    with urllib.request.urlopen(req) as resp:
        body = resp.read()
    return body if raw else json.loads(body or b"{}")


def get_release():
    return api(f"{API}/releases/tags/{TAG}")


def asset_text(asset_id):
    """Download an asset's raw bytes (used to read .sig files)."""
    body = api(
        f"{API}/releases/assets/{asset_id}",
        headers={"Accept": "application/octet-stream"},
        raw=True,
    )
    return body.decode().strip()


def main():
    release = get_release()
    release_id = release["id"]
    assets = {a["name"]: a for a in release.get("assets", [])}

    def find(suffix):
        # Returns (name, asset) for the first asset whose name ends with suffix.
        for name, a in assets.items():
            if name.endswith(suffix):
                return name, a
        return None, None

    def url_for(name):
        return f"https://github.com/{REPO}/releases/download/{TAG}/{name}"

    platforms = {}

    # macOS (Apple Silicon): <app>.app.tar.gz + .sig
    mac_sig_name, mac_sig = find(".app.tar.gz.sig")
    if mac_sig:
        bundle_name = mac_sig_name[: -len(".sig")]
        platforms["darwin-aarch64"] = {
            "signature": asset_text(mac_sig["id"]),
            "url": url_for(bundle_name),
        }
        print(f"darwin-aarch64 -> {bundle_name}")
    else:
        print("WARNING: no .app.tar.gz.sig asset found — darwin-aarch64 omitted")

    # Windows (NSIS): <app>_x64-setup.exe + .sig
    win_sig_name, win_sig = find("-setup.exe.sig")
    if win_sig:
        exe_name = win_sig_name[: -len(".sig")]
        entry = {
            "signature": asset_text(win_sig["id"]),
            "url": url_for(exe_name),
        }
        # Tauri's updater looks up "windows-x86_64"; the "-nsis" alias keeps
        # older clients working too.
        platforms["windows-x86_64"] = entry
        platforms["windows-x86_64-nsis"] = entry
        print(f"windows-x86_64 -> {exe_name}")
    else:
        print("WARNING: no -setup.exe.sig asset found — windows omitted")

    if not platforms:
        print("ERROR: no signed updater artifacts found in the release.", file=sys.stderr)
        sys.exit(1)

    latest = {
        "version": VERSION,
        "notes": (release.get("body") or "").strip(),
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z"),
        "platforms": platforms,
    }
    payload = json.dumps(latest, indent=2).encode()

    # Replace any existing latest.json (delete then upload)
    existing = assets.get("latest.json")
    if existing:
        api(f"{API}/releases/assets/{existing['id']}", method="DELETE")
        print("Deleted existing latest.json")

    api(
        f"https://uploads.github.com/repos/{REPO}/releases/{release_id}/assets?name=latest.json",
        method="POST",
        data=payload,
        headers={"Content-Type": "application/json"},
    )
    print(f"Uploaded latest.json with platforms: {', '.join(platforms)}")


if __name__ == "__main__":
    try:
        main()
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code} for {e.url}: {e.read().decode(errors='replace')}", file=sys.stderr)
        sys.exit(1)
