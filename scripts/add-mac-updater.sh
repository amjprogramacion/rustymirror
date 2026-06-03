#!/usr/bin/env bash
# Ensures the macOS updater artifact (.app.tar.gz + .app.tar.gz.sig) exists in the
# GitHub release and that latest.json includes the darwin-aarch64 platform entry.
#
# Required env vars: GITHUB_TOKEN, TAURI_SIGNING_PRIVATE_KEY,
#                    TAURI_SIGNING_PRIVATE_KEY_PASSWORD, TAG, REPO
set -euo pipefail

BUNDLE_DIR="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"
VERSION="${TAG#v}"

APP_PATH=$(find "$BUNDLE_DIR" -maxdepth 1 -name "*.app" 2>/dev/null | head -1)
if [ -z "$APP_PATH" ]; then
  echo "No .app bundle found in $BUNDLE_DIR — skipping"
  exit 0
fi

APP_NAME=$(basename "$APP_PATH" .app)
TARBALL="${APP_NAME}_${VERSION}_aarch64.app.tar.gz"
TARBALL_PATH="${BUNDLE_DIR}/${TARBALL}"
SIG_PATH="${TARBALL_PATH}.sig"

# Create tarball if the Tauri bundler did not produce it
if [ ! -f "$TARBALL_PATH" ]; then
  echo "Creating $TARBALL..."
  (cd "$BUNDLE_DIR" && tar czf "$TARBALL" "${APP_NAME}.app")
fi

# Sign if not already signed (TAURI_SIGNING_PRIVATE_KEY is read from env by the CLI)
if [ ! -f "$SIG_PATH" ]; then
  echo "Signing $TARBALL..."
  npx tauri signer sign "$TARBALL_PATH"
fi

# Fetch release metadata
RELEASE_JSON=$(curl -sf -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/${REPO}/releases/tags/${TAG}")
RELEASE_ID=$(echo "$RELEASE_JSON" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Upload tarball + sig if not already in the release
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

# Find the existing latest.json asset
LATEST_ID=$(echo "$RELEASE_JSON" | python3 -c \
  "import sys,json; a=json.load(sys.stdin).get('assets',[]); print(next((str(x['id']) for x in a if x['name']=='latest.json'),''))")
if [ -z "$LATEST_ID" ]; then
  echo "latest.json not found in release — skipping"
  exit 0
fi

# Download latest.json to a temp file
curl -sL -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/octet-stream" \
  "https://api.github.com/repos/${REPO}/releases/assets/${LATEST_ID}" \
  -o /tmp/latest.json

# Inject darwin-aarch64 — all data passed via env to avoid shell-in-Python quoting issues
SIG_PATH="$SIG_PATH" TARBALL_URL="$TARBALL_URL" python3 << 'PYEOF'
import json, os
sig_path   = os.environ['SIG_PATH']
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

# Replace the asset: delete old, upload new
curl -sf -X DELETE -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/${REPO}/releases/assets/${LATEST_ID}"
curl -sf -X POST -H "Authorization: token $GITHUB_TOKEN" \
  -H "Content-Type: application/json" --data-binary @/tmp/latest.json \
  "https://uploads.github.com/repos/${REPO}/releases/${RELEASE_ID}/assets?name=latest.json" > /dev/null

echo "Done — darwin-aarch64 added to latest.json"
