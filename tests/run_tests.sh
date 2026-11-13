#!/usr/bin/env bash
# Safe Linux real-binary integration suite (stdlib Python; no global process kills).
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/linux_integration.py" "$@"
