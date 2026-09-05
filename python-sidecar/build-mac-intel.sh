#!/usr/bin/env bash
# python-sidecar/build-mac-intel.sh
#
# One-command macOS (Intel) build for the offline speech sidecar.
# Reuses .venv if present; safe to re-run. Every gate must pass.
#
#   bash build-mac-intel.sh

set -euo pipefail
cd "$(dirname "$0")"

echo "==> [1/7] Air-gap build environment"
export HF_HUB_OFFLINE=1
export TRANSFORMERS_OFFLINE=1
export HF_DATASETS_OFFLINE=1
export HF_HUB_DISABLE_TELEMETRY=1

echo "==> [2/7] Python 3.11 venv"
if [ ! -x .venv/bin/python ]; then
  PY11="$(command -v python3.11 || echo /usr/local/opt/python@3.11/bin/python3.11)"
  "$PY11" -m venv .venv
fi
.venv/bin/python --version

echo "==> [3/7] Installing requirements (CPU torch — Intel Macs have no CUDA)"
.venv/bin/pip install --quiet --upgrade pip
.venv/bin/pip install --quiet torch torchaudio
.venv/bin/pip install --quiet -r requirements.txt

echo "==> [4/7] Verifying offline environment"
.venv/bin/python verify_env.py
echo "    (device=cpu is the EXPECTED configuration on Intel Macs)"

echo "==> [5/7] Mock pipeline test"
.venv/bin/python test_mock_run.py

echo "==> [6/7] PyInstaller freeze (onedir, ~2-4 GB, be patient)"
.venv/bin/python -m PyInstaller audio_sidecar_macos.spec --noconfirm

DIST="dist/audio-processor-x86_64-apple-darwin"
if [ ! -x "$DIST/audio-processor-x86_64-apple-darwin" ]; then
  echo "ERROR: frozen binary not found at $DIST" >&2
  exit 1
fi

echo "==> [7/7] Ad-hoc codesign + smoke tests on the frozen binary"
codesign --force --deep --sign - "$DIST/audio-processor-x86_64-apple-darwin"

# stdin protocol smoke test
PROTO_OUT="$(printf '{"command":"PING"}\n{"command":"EXIT"}\n' \
  | "$DIST/audio-processor-x86_64-apple-darwin")"
echo "$PROTO_OUT" | grep -q '"event": "READY"' || { echo "FAIL: no READY" >&2; exit 1; }
echo "$PROTO_OUT" | grep -q '"event": "PONG"'  || { echo "FAIL: no PONG" >&2;  exit 1; }
echo "$PROTO_OUT" | grep -q '"event": "BYE"'   || { echo "FAIL: no BYE" >&2;   exit 1; }
echo "    stdin protocol OK"

# mock pipeline smoke test
TMPDIR_SMOKE="$(mktemp -d)"
"$DIST/audio-processor-x86_64-apple-darwin" \
  --audio /dev/null --output-dir "$TMPDIR_SMOKE" --session-id smoke-1 --mock \
  | grep -q '"event": "COMPLETE"' || {
    echo "FAIL: frozen binary mock run" >&2; exit 1;
  }
[ -f "$TMPDIR_SMOKE/transcript.json" ] || { echo "FAIL: no transcript.json" >&2; exit 1; }
[ -f "$TMPDIR_SMOKE/summary.md" ]      || { echo "FAIL: no summary.md" >&2; exit 1; }
[ -f "$TMPDIR_SMOKE/metadata.json" ]   || { echo "FAIL: no metadata.json" >&2; exit 1; }
echo "    frozen mock pipeline OK"

# Stage: the onedir dist stays in python-sidecar/dist/ — tauri.conf.json
# bundles it via `bundle.resources` ("sidecar/" under Contents/Resources).
# Nothing is copied into src-tauri/binaries anymore.
echo ""
echo "SIDECAR READY: $DIST/"
echo "Next: cd ../src-tauri && npm run tauri build"