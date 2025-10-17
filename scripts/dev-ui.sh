#!/usr/bin/env bash

set -euo pipefail

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for agentdev dev workflow. Please install tmux first." >&2
  exit 1
fi

SESSION_NAME=${AGENTDEV_DEV_SESSION:-agentdev_dev}

if tmux has-session -t "${SESSION_NAME}" 2>/dev/null; then
  echo "tmux session \"${SESSION_NAME}\" already exists."
  read -r -p "Kill existing session? [y/N] " kill_session
  case "${kill_session}" in
    [yY][eE][sS]|[yY])
      tmux kill-session -t "${SESSION_NAME}"
      ;;
    *)
      echo "Attach with: tmux attach -t ${SESSION_NAME} or rerun after closing the session." >&2
      exit 1
      ;;
  esac
fi

REPO_ROOT=$(git rev-parse --show-toplevel)
export REPO_ROOT
export DEV_DIST_DIR=.next-dev
BACKEND_PORT=${AGENTDEV_BACKEND_PORT:-3000}
FRONTEND_PORT=${AGENTDEV_FRONTEND_PORT:-3100}
# Default the API base to 127.0.0.1 to avoid IPv6 localhost collisions.
DEFAULT_API_BASE="http://127.0.0.1:${BACKEND_PORT}"
API_BASE=${AGENTDEV_API_BASE:-${DEFAULT_API_BASE}}

check_port_free() {
  local port=$1
  if lsof -i :"${port}" >/dev/null 2>&1; then
    echo "Port ${port} is already in use by:"
    lsof -i :"${port}"
    echo
    if [[ -t 0 && -t 1 ]]; then
      read -r -p "Kill these processes? [y/N] " answer || answer=""
      case "${answer}" in
        [yY][eE][sS]|[yY])
          # Extract PIDs (NR>1 skips header)
          lsof -ti :"${port}" | uniq | xargs -r kill
          sleep 1
          if lsof -i :"${port}" >/dev/null 2>&1; then
            echo "Failed to free port ${port}. Please resolve manually." >&2
            exit 1
          fi
          ;;
        *)
          echo "Cannot continue while port ${port} is occupied." >&2
          exit 1
          ;;
      esac
    else
      echo "Cannot prompt to free port ${port} in non-interactive mode." >&2
      echo "Stop the process above or rerun with AGENTDEV_BACKEND_PORT/AGENTDEV_FRONTEND_PORT." >&2
      exit 1
    fi
  fi
}

check_port_free "${BACKEND_PORT}"
check_port_free "${FRONTEND_PORT}"

# Clear stale Next.js build artifacts to avoid mismatched server chunks when
# switching between `next build` (used for embedding) and `next dev`.
python3 - <<'PY'
import os
import shutil
from pathlib import Path

repo_root = Path(os.environ["REPO_ROOT"])
dist_dir = os.environ.get("DEV_DIST_DIR", ".next-dev")
build_dir = repo_root / "apps" / "frontend" / dist_dir
if build_dir.exists():
    shutil.rmtree(build_dir)
PY

tmux new-session -d -s "${SESSION_NAME}" -c "${REPO_ROOT}" \
  "PORT=${BACKEND_PORT} AGENTDEV_BACKEND_PORT=${BACKEND_PORT} AGENTDEV_SKIP_UI_BUILD=1 cargo run --manifest-path apps/backend/Cargo.toml --bin agentdev-ui"

tmux split-window -h -t "${SESSION_NAME}:0" -c "${REPO_ROOT}/apps/frontend" \
  "PORT=${FRONTEND_PORT} AGENTDEV_FRONTEND_PORT=${FRONTEND_PORT} NEXT_PUBLIC_AGENTDEV_API_BASE=${API_BASE} pnpm run dev"

tmux select-layout -t "${SESSION_NAME}:0" even-horizontal
tmux set-option -t "${SESSION_NAME}" remain-on-exit on

cat <<EOF
Started agentdev dev session "${SESSION_NAME}".
Attach with: tmux attach -t ${SESSION_NAME}
Backend: http://localhost:${BACKEND_PORT}
Frontend dev server: http://localhost:${FRONTEND_PORT}
EOF
