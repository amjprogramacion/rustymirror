#!/usr/bin/env bash
# Ensures the macOS updater artifact (.app.tar.gz + .app.tar.gz.sig) exists in the
# GitHub release and that latest.json includes the darwin-aarch64 platform entry.
#
# Required env vars: GITHUB_TOKEN, TAURI_SIGNING_PRIVATE_KEY,
#                    TAURI_SIGNING_PRIVATE_KEY_PASSWORD, TAG, REPO
set -euo pipefail

TARGET_BASE="src-tauri/target"
VERSION="${TAG#v}"

# ── Locate .app.tar.gz or .app bundle ────────────────────────────────────────
# Prefer an already-created tarball (bundler may produce it with createUpdaterArtifacts).
# grep -v is intentionally avoided in these pipelines: on empty find output it exits 1
# and would trip set -o pipefail.
TARBALL_PATH=$(find "$TARGET_BASE" -name "*.app.tar.gz" 2>/dev/null | head -1)

if [ -n "$TARBALL_PATH" ]; then
  echo "Found existing tarball: $TARBALL_PATH"
  TARBALL=$(basename "$TARBALL_PATH")
  SIG_PATH="${TARBALL_PATH}.sig"
else
  # Fall back to locating the .app directory and creating the tarball ourselves
  APP_PATH=$(find "$TARGET_BASE" -name "*.app" -type d 2>/dev/null | head -1)

  if [ -z "$APP_PATH" ]; then
    echo "DEBUG: Searching for bundle output directories..."
    find "$TARGET_BASE" -name "bundle" -type d 2>/dev/null | while read -r d; do
      echo "  $d:"
      ls -la "$d" 2>/dev/null || true
    done
    echo "No .app bundle or .app.tar.gz found — skipping"
    exit 0
  fi

  echo "Found .app bundle: $APP_PATH"
  APP_NAME=$(basename "$APP_PATH" .app)
  BUNDLE_DIR=$(dirname "$APP_PATH")
  TARBALL="${APP_NAME}_${VERSION}_aarch64.app.tar.gz"
  TARBALL_PATH="${BUNDLE_DIR}/${TARBALL}"
  SIG_PATH="${TARBALL_PATH}.sig"

  echo "Creating $TARBALL..."
  (cd "$BUNDLE_DIR" && tar czf "$TARBALL" "${APP_NAME}.app")
fi

# ── Sign if not already signed ────────────────────────────────────────────────
# TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD are read
# automatically from the environment by the Tauri CLI.
if [ ! -f "$SIG_PATH" ]; then
  echo "Signing $(basename "$TARBALL_PATH")..."
  npx tauri signer sign "$TARBALL_PATH"
fi

TARBALL=$(basename "$TARBALL_PATH")

# ── Fetch release metadata ────────────────────────────────────────────────────
RELEASE_JSON=$(curl -sf -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/${REPO}/releases/tags/${TAG}")
RELEASE_ID=$(echo "$RELEASE_JSON" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# ── Upload tarball + sig if missing from the release ─────────────────────────
ASSET_NAMES=$(echo "$RELEASE_JSON" | python3 -c \
  "import sys,json; print(' '.join(a['name'] for a in json.load(sys.stdin).get('assets',[])))")
if [[ "$ASSET_NAMES" != *"$TARBALL"* ]]; then
  echo "Uploading $TARBALL..."
  curl -sf -X POST -H "Authorization: token $GITHUB_TOKEN" \
    -H "Content-Type: application/gzip" --data-binary @"$TARBALL_PATH" \
    "https://uploads.github.com/repos/${REPO}/releases/${RELEASE_ID}/assets?name=${TARBALL}" > /dev/null
  curl -sf -X POST -H "Authorization: token $GITHUB_TOKEN" \
    -H "Content-Type: text/plain" --data-binary @"$SIG_PATH" \
    "https://uploads.github.com/repos/${REPO}/releases/${RELEASE_ID}/assets?name=${TARBALL}.sig" > /dev/null
fi

TARBALL_URL="https://github.com/${REPO}/releases/download/${TAG}/${TARBALL}"

# ── Find latest.json asset ────────────────────────────────────────────────────
LATEST_ID=$(echo "$RELEASE_JSON" | python3 -c \
  "import sys,json; a=json.load(sys.stdin).get('assets',[]); print(next((str(x['id']) for x in a if x['name']=='latest.json'),''))")
if [ -z "$LATEST_ID" ]; then
  echo "latest.json not found in release — skipping"
  exit 0
fi

# ── Download latest.json, inject darwin-aarch64, re-upload ───────────────────
curl -sL -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/octet-stream" \
  "https://api.github.com/repos/${REPO}/releases/assets/${LATEST_ID}" \
  -o /tmp/latest.json

SIG_PATH="$SIG_PATH" TARBALL_URL="$TARBALL_URL" python3 << 'PYEOF'
import json, os
sig_path    = os.environ['SIG_PATH']
tarball_url = os.environ['TARBALL_URL']
with open(sig_path) as f:
    sig = f.read().strip()
with open('/tmp/latest.json') as f:
    data = json.load(f)
platforms = data.get('platforms') or {}
platforms['darwin-aarch64'] = dict(signature=sig, url=tarball_url)
data['platforms'] = platforms
with open('/tmp/latest.json', 'w') as f:
    json.dump(data, f, indent=2)
PYEOF

curl -sf -X DELETE -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/${REPO}/releases/assets/${LATEST_ID}"
curl -sf -X POST -H "Authorization: token $GITHUB_TOKEN" \
  -H "Content-Type: application/json" --data-binary @/tmp/latest.json \
  "https://uploads.github.com/repos/${REPO}/releases/${RELEASE_ID}/assets?name=latest.json" > /dev/null

echo "Done — darwin-aarch64 added to latest.json"
