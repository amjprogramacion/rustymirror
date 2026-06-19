#!/usr/bin/env bash
# Ensures the macOS updater artifact (.app.tar.gz + .app.tar.gz.sig) exists in the
# GitHub release and that latest.json includes the darwin-aarch64 platform entry.
#
# Required env vars: GITHUB_TOKEN, TAURI_SIGNING_PRIVATE_KEY,
#                    TAURI_SIGNING_PRIVATE_KEY_PASSWORD, TAG, REPO
# Optional:          ARTIFACT_PATHS (JSON array from tauri-action's artifactPaths output)
set -uo pipefail

VERSION="${TAG#v}"

# ── Diagnostics: show exactly what tauri-action produced ─────────────────────
echo "::group::Bundle diagnostics"
echo "pwd: $(pwd)"
echo "ARTIFACT_PATHS: ${ARTIFACT_PATHS:-<unset>}"
echo "--- All .app / .app.tar.gz / .dmg / .sig under workspace ---"
find . -type d -name "node_modules" -prune -o \
  \( -name "*.app" -o -name "*.app.tar.gz" -o -name "*.dmg" -o -name "*.sig" \) -print 2>/dev/null
echo "::endgroup::"

# ── Locate the .app.tar.gz (preferred) or .app bundle ────────────────────────
# Search the whole workspace; a plain string is captured (no head in the pipe so
# an empty result can't trip pipefail). Take the first line in pure bash.
ALL_TARBALLS=$(find . -type d -name "node_modules" -prune -o -name "*.app.tar.gz" -print 2>/dev/null)
TARBALL_PATH="${ALL_TARBALLS%%$'\n'*}"

if [ -n "$TARBALL_PATH" ]; then
  echo "Found existing tarball: $TARBALL_PATH"
  SIG_PATH="${TARBALL_PATH}.sig"
else
  ALL_APPS=$(find . -type d -name "node_modules" -prune -o -type d -name "*.app" -print 2>/dev/null)
  APP_PATH="${ALL_APPS%%$'\n'*}"

  if [ -z "$APP_PATH" ]; then
    echo "No .app bundle or .app.tar.gz found anywhere — cannot build updater artifact."
    echo "Inspect the diagnostics above to locate the bundle, then adjust this script."
    exit 1
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

TARBALL=$(basename "$TARBALL_PATH")

# ── Sign if not already signed ────────────────────────────────────────────────
# TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD are read
# automatically from the environment by the Tauri CLI.
if [ ! -f "$SIG_PATH" ]; then
  echo "Signing $TARBALL..."
  npx tauri signer sign "$TARBALL_PATH"
fi

# From here on a failure should fail the job.
set -e

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
