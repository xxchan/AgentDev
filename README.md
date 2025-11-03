# agentdev

Features
- Worktree 管理多 agent 并行开发
- 多 agent 跑同一个任务赛马🏇
- Worktrees / Sessions UI

## 安装

[Install Rust](https://www.rust-lang.org/tools/install)

```bash
cargo install --git https://github.com/xxchan/AgentDev agentdev
```

## 配置

在 `~/.config/agentdev/config.toml` 配置 agent 池。参考默认配置里的说明。
或者参考 [config.example.toml](https://github.com/xxchan/AgentDev/blob/main/config.example.toml)

## 使用

### UI

```bash
agentdev ui --port 9999
```

- /sessions 页面: 查看本地所有 agent sessions（不依赖）
- /worktrees 页面: 查看 agentdev 管理的 worktrees 里的 agent sessions / git diff

### Worktree-driven local parallel development

```bash
# create a worktree, and start an agent session
agentdev wt create

# Run a command in a worktree, e.g., `pnpm dev`, `code .`
agentdev wt exec <cmd>

# Merge worktree into main / delete worktree
agentdev wt [merge|delete] <worktree>
```

### 并行多 Agent 赛马（TODO）

```bash
agentdev start "研究一下这个项目，把介绍写到一个文件里"
# 只选部分 Agent，以及显式指定任务名
agentdev start "研究一下这个项目，把介绍写到一个文件里" --agents claude,codex --name research

agentdev delete-task <task>
```
