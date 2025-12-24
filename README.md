# agentdev

Features
- Git worktree management for parallel, multi-agent development
- Worktrees / Sessions UI for inspecting local runs
- Supports ANY CLI coding agents
    + Sessions UI currently supports: [Kimi CLI](https://github.com/MoonshotAI/kimi-cli), Codex, Claude Code

Blog
- Chinese: [使用 Local Coding Agents 疯狂地并发开发](https://xxchan.me/zh/blog/2025-11-14-concurrent-local-coding-agents/)
- English: [Concurrent Local Coding Agents](https://xxchan.me/blog/2025-11-14-concurrent-local-coding-agents/index_en/)

## Installation

Prerequisites:
- [Install Rust](https://www.rust-lang.org/tools/install)
- Install `pnpm`

```bash
cargo install --git https://github.com/xxchan/AgentDev agentdev
```

## Configuration

Configure your agent pool in `~/.config/agentdev/config.toml`. Use the comments in the default file for guidance or consult [config.example.toml](https://github.com/xxchan/AgentDev/blob/main/config.example.toml).

## Usage

### UI

```bash
agentdev ui --port 9999
```

- `/sessions`: inspect every local agent session (works without agentdev)
- `/worktrees`: inspect agentdev-managed worktrees, their agent sessions, and git diffs

### Worktree-driven local parallel development

```bash
# Create a worktree and start an agent session
agentdev wt create

# Run a command inside a worktree, e.g., `pnpm dev`, `code .`
agentdev wt exec <cmd>

# Merge a worktree back to main or delete it
agentdev wt [merge|delete] <worktree>

# Inspect everything agentdev is tracking
agentdev wt list
```

