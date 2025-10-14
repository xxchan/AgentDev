#!/usr/bin/env bash

set -euo pipefail

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for agentdev dev workflow. Please install tmux first." >&2
  exit 1
fi

SESSION_NAME=${AGENTDEV_DEV_SESSION:-agentdev_dev}

if tmux has-session -t "${SESSION_NAME}" 2>/dev/null; then
  echo "tmux session \"${SESSION_NAME}\" already exists. Attach with: tmux attach -t ${SESSION_NAME}" >&2
  exit 1
fi

REPO_ROOT=$(git rev-parse --show-toplevel)
BACKEND_PORT=${AGENTDEV_BACKEND_PORT:-3000}
FRONTEND_PORT=${AGENTDEV_FRONTEND_PORT:-3100}
API_BASE=${AGENTDEV_API_BASE:-http://localhost:${BACKEND_PORT}}

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
