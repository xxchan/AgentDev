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
    read -r -p "Kill these processes? [y/N] " answer
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
  fi
}

check_port_free "${BACKEND_PORT}"
check_port_free "${FRONTEND_PORT}"

tmux new-session -d -s "${SESSION_NAME}" -c "${REPO_ROOT}" \
  "PORT=${BACKEND_PORT} cargo run --manifest-path apps/backend/Cargo.toml --bin agentdev-ui"

tmux split-window -h -t "${SESSION_NAME}:0" -c "${REPO_ROOT}/apps/frontend" \
  "PORT=${FRONTEND_PORT} NEXT_PUBLIC_AGENTDEV_API_BASE=${API_BASE} pnpm run dev"

tmux select-layout -t "${SESSION_NAME}:0" even-horizontal
tmux set-option -t "${SESSION_NAME}" remain-on-exit on

cat <<EOF
Started agentdev dev session "${SESSION_NAME}".
Attach with: tmux attach -t ${SESSION_NAME}
Backend: http://localhost:${BACKEND_PORT}
Frontend dev server: http://localhost:${FRONTEND_PORT}
EOF
