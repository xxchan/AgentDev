# RFC: 会话一键触发 MVP（复用 `/commands`）

## 背景与问题

AgentDev Web UI 目前只能浏览存量会话，继续对话需要用户手动在命令行运行 `claude resume <session_id>`，体验割裂。我们希望在 UI 内直接完成「继续会话」或「新建 worktree + 会话」的场景，同时维持系统的简洁：不新增长连接、不引入复杂的消息推送。

现有基础设施包括：

- `/api/worktrees/:id/commands`：包装 `agentdev worktree exec` 的一次性命令执行接口，执行过程由 `ProcessRegistry` 记录 stdout/stderr 和状态。
- 会话列表 / 详情 API：UI 通过轮询拉取会话历史（由 CLI 在磁盘落盘）。
- Worktree 进程面板：展示最近命令记录与日志。

目标是复用上述能力，让 UI 触发 Claude CLI 后，只需等待下一次会话轮询即可看到新增内容。

## 设计原则

1. **大道至简**：不新增 `/messages` 或流式推送，命令执行链路保持为「POST `/commands` → 记录进程 → CLI 自行落盘」。
2. **最小侵入**：UI 只拼好命令并调用已有 API；Session 仍靠轮询刷新，不绑定命令执行过程。
3. **渐进增强**：若需要更丰富的状态提示，通过扩展 `ProcessRegistry` 元数据实现，避免再造一套专用接口。
4. **复用现有日志**：stdout/stderr、本地错误都已经在 `/commands` 链路中捕获，直接向用户呈现。

## 方案概述

- **继续会话**：前端按钮将 prompt 与 session/worktree 信息拼成 `claude resume <session_id> "<prompt>"` 命令，通过 `/api/worktrees/:id/commands` 发送。命令执行完成后，UI 依赖会话轮询获取新增消息；若命令失败，则在 Worktree 进程面板中查看 stderr。
- **新建 worktree + 会话**：先调用既有（或补充的）“创建 worktree”接口生成分支，再立即通过 `/commands` 启动 `claude resume`。首条消息仍由 CLI 落盘，UI 通过轮询拿结果。
- **状态增强（可选）**：为 `ProcessRecord` 增添 `session_provider`、`session_id`、`prompt_preview` 字段，使 UI 能从 `/worktrees/:id/processes` 推导出某个 session 是否正在执行，并在会话列表中显示“处理中/失败”等提示。

## 前端改动要点

### 继续聊天按钮

- 文件：`apps/frontend/features/command/components/ResumeCommandButton.tsx`
- 行为调整：
  - 改为真正触发 `useLaunchWorktreeCommand` mutation，不再只复制命令。
  - 由按钮传入 `worktreeId`、`sessionId`、`prompt`，内部拼装 CLI 字符串。
  - Mutation 成功后失效 `queryKeys.worktrees.processes` 与 `queryKeys.sessions.list`。
  - 本地状态展示 loading/success/error，并在失败时引导用户查看进程日志。

### Sessions 页全局补话

- 在 `apps/frontend/app/sessions/page.tsx` 新增 composer：
  - 选择目标 session（或手动输入 ID）和关联 worktree。
  - 调用同一 `useLaunchWorktreeCommand`，复用按钮逻辑。
  - 异常提示保持轻量，主刷新仍依赖会话轮询。

### 进程状态辅助（增强阶段）

- 在 `SessionListView` 渲染时，额外读取 `WorktreeProcessSummary` 映射，生成“等待 Claude 输出…”或“命令失败”提示。
- 使用 `WorktreeProcesses` 面板现成数据展示 stdout/stderr。

## 后端改动要点

### `/api/worktrees/:id/commands` 复用

- 仍由 `launch_worktree_command` 创建 `ProcessRecord` 并启动线程执行命令。
- 若要识别会话上下文，扩展：
  - `ProcessRecord` 增加 Session 元信息字段；
  - 允许 `LaunchWorktreeCommandRequest` 携带可选 `session_provider` / `session_id` / `prompt_preview`；
  - 在 `process_record_to_summary` 中返回这些字段，供前端判断状态。
- 其它逻辑完全沿用现有实现：stdout/stderr 捕获、失败标记、队列长度控制等无需修改。

### Worktree 创建

- 若 UI 需要“一键创建 worktree 并启动会话”，可在后端添加薄薄的封装：
  - 调用 `handle_create_in_dir_quiet` 建立新 worktree；
  - 将新建结果写回 `XlaudeState`；
  - 再用 `/commands` 启动首条 Resume 命令。
- 这层封装与本文核心方案解耦，可根据时间安排。

## 流程梳理

### 继续会话

1. 用户在 UI 按钮输入 prompt，点击提交。
2. 前端调用 `/api/worktrees/:id/commands`，命令为 `claude resume ...`。
3. 后端写入 `ProcessRegistry`（状态 `pending`），返回 `LaunchWorktreeCommandResponse`。
4. 线程执行命令，stdout/stderr 被捕获，状态最终更新为 `succeeded/failed`。
5. UI 轮询 `/api/sessions` / `/api/sessions/:provider/:session_id`，收到最新记录后渲染。
6. 若命令失败，用户可在 Worktree Processes 面板查看错误日志，按钮提示失败状态。

### 新建 worktree + 触发首条消息

1. 用户填写仓库/分支/名称及初始 prompt。
2. 后端创建 worktree，写入状态。
3. 通过 `/commands` 触发 `claude resume`。
4. 后续与“继续会话”流程一致，UI 通过轮询获取首条消息。

## 风险与缓解

- **命令拼接转义**：前端在拼命令时需对 prompt 做引号转义。可封装 util（例如 `buildClaudeResumeCommand`）确保一致性。
- **会话定位失败**：用户仅凭 sessionId 发起命令，若未指定 worktree，后端可能找不到路径。MVP 先在 UI 阻止这种情况；后续可在 `/commands` 对 provider/session 做校验并返回 400。
- **状态提示缺失**：MVP 阶段 UI 只靠轮询可能感知不到“正在运行”。随着 `ProcessRecord` 元数据补齐，逐步增强提示逻辑。

## 后续演进

- 在 `ProcessRegistry` 中记录 session 元信息，前端即可在会话列表中显示命令状态。
- 封装 worktree 创建 + 初始 resume 的 API，完成「一键创建并开聊」体验。
- 若未来需要跨 provider 差异化处理（如 Kimi/Kimi CLI），也可以复用此模式：各 provider 的 resume 命令在前端/后端统一拼装即可。

