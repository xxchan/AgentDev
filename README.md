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

- 启动一个任务（并行多 Agent）

```bash
agentdev start "研究一下这个项目，把介绍写到一个文件里"
# 只选部分 Agent，以及显式制定任务名
agentdev start "研究一下这个项目，把介绍写到一个文件里" --agents claude,codex --name research
```

- 每个 worktree 默认会创建在 `../<repo>.worktrees/<worktree>` 目录下，方便在主仓旁集中管理。

- 启动 Web UI 查看 / 对比所有 agent

```bash
agentdev ui  # 启动前端 + 后端服务，浏览器访问 http://localhost:3100
```
Sessions 页面支持对比分支、查看 git diff、发 follow-up prompt。`agentdev start` 会在 tmux 中启动各个 agent，会话仍可通过 `tmux attach` 继续。

- 一键清理整组任务

```bash
agentdev delete-task <task>
```

## UI

- /sessions 页面: 查看本地所有 agent sessions（不依赖）
- /worktrees 页面: 查看 agentdev 管理的 worktrees 里的 agent sessions / git diff
