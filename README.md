# agentdev

一个用来快速对比多种 AI Agent 方案（agent 赛马🏇）的极简工具：一条命令启动任务，仪表盘里横向对比，满意就附着继续干，最后一键清理。

## 安装

[Install Rust](https://www.rust-lang.org/tools/install)

```bash
cargo install --git https://github.com/xxchan/AgentDev
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

- 仪表盘查看与对比（需要 tmux）

```bash
agentdev dashboard  # 左侧按任务分组；右侧显示 Initial prompt、分层 diff；q 退出；f 给所有 agent 发送 follow-up prompt
```
选中 worktree 后按 Enter attach tmux session，可以继续和 agent 对话，Ctrl+Q 返回。

- 一键清理整组任务（或者在 dashboard 上用 d 删除）

```bash
agentdev delete-task <task>
```

## 开发仪表盘 / UI

仓库内置了前后端联调脚本，默认会在一个 tmux 会话里同时启动 Rust 后端和 Next.js 开发服务器，刷新即热加载。

```bash
# 项目根目录
pnpm install              # 首次需要安装前端依赖
pnpm run dev:ui           # 创建 agentdev_dev tmux 会话
tmux attach -t agentdev_dev   # 查看日志或交互，CTRL+B D 可分离
```

- 后端监听 `http://localhost:3000`，前端 dev server 监听 `http://localhost:3100`，UI 内发往 `/api/*` 的请求会自动代理到后端。
- 可通过环境变量调整端口或代理地址：
  - `AGENTDEV_BACKEND_PORT`（默认 `3000`）
  - `AGENTDEV_FRONTEND_PORT`（默认 `3100`）
  - `AGENTDEV_API_BASE`（默认 `http://localhost:<AGENTDEV_BACKEND_PORT>`）
- 退出时记得 `tmux kill-session -t agentdev_dev` 或在 tmux 内 `exit`，避免残留进程占用端口。

生产构建仍然使用静态导出：

```bash
pnpm run build:frontend   # 生成 apps/frontend/out 静态资源
cargo build --release     # 构建带内嵌 dashboard 的后端
```
