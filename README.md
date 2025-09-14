# agentdev — 最简上手

一个用来快速对比多种 AI Agent 方案的极简工具：一条命令启动任务，仪表盘里横向对比，满意就附着继续干，最后一键清理。

## 安装

```bash
cargo install --git https://github.com/xxchan/AgentDev
```

## 配置

在 `~/.config/agentdev/config.toml` 配置 agent 池。

查看 `agentdev --help`

## 使用

- 启动一个任务（并行多 Agent）

```bash
agentdev start "研究一下这个项目，把介绍写到一个文件里"
# 只选部分 Agent：
agentdev start "研究一下这个项目，把介绍写到一个文件里" --agents claude,codex
```

- 仪表盘查看与对比（需要 tmux）

```bash
agentdev dashboard  # 左侧按任务分组，右侧分层 diff，q 退出
```
选中 worktree 后按 Enter attach tmux session，可以继续和 agent 对话，Ctrl+Q 返回。

- 一键清理整组任务（或者在 dashboard 上用 d 删除）

```bash
agentdev delete-task <task>
```
