### **第一部分：最终需求 (The "What")**

**项目目标**
创建一个名为 `agentdev` 的命令行工具，旨在帮助开发者对多个不同的命令行 AI Agent（如 Claude Code, Codex 等）在同一个任务上的表现进行并排的、直观的“体感对比 (vibe eval)”。

**核心工作流**
1.  **配置 (Configure)**: 用户在一个全局配置文件 `~/.config/agentdev/config.toml` 中定义一个“Agent 池”，为每个 Agent 指定一个别名和其完整的启动命令。
2.  **启动 (Start)**: 用户通过 `agentdev start "<prompt>"` 来启动一次评测。
3.  **自动化准备 (Automate)**: 工具在后台自动为每个已配置的 Agent 执行以下操作：
    *   基于一个随机生成的任务名（如 `electric-purple-donkey`），创建一个独立的 `git worktree`（如 `electric-purple-donkey-claude`）。
    *   在该 worktree 内，启动一个分离的 `tmux` 会话，并运行对应的 Agent 命令。
    *   通过 `tmux send-keys` 将初始任务指令发送给 Agent，让其开始工作。
> 注：早期版本的终端仪表盘已废弃，现统一使用 Web UI。

4.  **评估 (Evaluate)**: 用户运行 `agentdev ui`，进入一个基于浏览器的仪表盘。
    *   **任务分组**: 仪表盘左侧以“任务”为单位对所有 worktree 进行分组展示，形成一个可折叠的树状列表。
    *   **分层 Diff 预览**: 仪表盘右侧的预览窗格会根据用户的选择动态变化：
        *   **宏观对比**: 当用户选中一个**任务节点**（如 `electric-purple-donkey`）时，预览区会显示一个**合并后**的 `git diff` 视图，将该任务下所有 Agent 产生的代码变更并排展示，方便快速横向比较。
        *   **微观审查**: 当用户展开任务并选中一个具体的 **Agent 节点**（如 `claude`）时，预览区仅显示**这一个 Agent** 的 `git diff`，方便深入审查其具体方案。
5.  **深入 (Engage)**: 在仪表盘中，用户可以选中任何一个满意的 Agent，按 `Enter` 键即可无缝附着（attach）到其 `tmux` 会话中，继续进行交互式开发。
6.  **清理 (Clean Up)**: 任务完成后，用户可运行 `agentdev delete-task <task_name>`，一键式地、安全地删除与该任务相关的所有 worktree、`tmux` 会话及状态记录。也可以在 dashboard 上用快捷键 `d` 删除 task / worktree

**技术基础**
该项目将作为现有 Rust 项目 `xlaude` 的一个新功能分支进行开发，以最大化复用其在 `git worktree` 管理、`tmux` 交互和 TUI 框架上的成熟能力。

---

### **第二部分：高层任务分解 (The "How" - Abstract)**

为了实现上述需求，我们将项目分解为以下六个核心任务：

1.  **Task 1: Agent 配置管理**: 建立从 `config.toml` 文件加载 Agent 定义的机制。
2.  **Task 2: 状态管理扩展**: 改造 `xlaude` 的状态文件结构，引入“任务 (`task_id`)”概念，以关联多个 worktree。
3.  **Task 3: 实现 `agentdev start` 命令**: 构建核心的评测任务启动逻辑，包括批量创建 worktree 和 `tmux` 会话。
4.  **Task 4: Dashboard UI 改造**: 将 `xlaude` 的扁平列表重构为按任务分组的树状视图。
5.  **Task 5: Dashboard "分层 Diff" 预览功能**: 实现仪表盘右侧窗格根据选择层级（任务 vs. Agent）动态显示不同 `diff` 的核心逻辑。
6.  **Task 6: 实现 `agentdev delete-task` 命令**: 提供一个便捷的命令来完成整个任务的资源清理。

---

### **第三部分：详细实施计划 (The "How" - Concrete)**

**环境设置**
1.  **创建分支**: 从 `xlaude` 项目 `main` 分支创建一个新分支 `feature/agentdev-poc`。
2.  **重命名项目**: 在 `Cargo.toml` 中将包名修改为 `agentdev`，确保编译产物为 `agentdev`。

**编码步骤**

*   **Step 1: Agent 配置管理 (Task 1)**
    1.  在 `src/config.rs` 中定义 `Config` 和 `AgentProfile` 结构体，使用 `serde` 和 `toml` 进行解析。
    2.  实现 `load_config()` 函数，该函数会尝试从 `~/.config/agentdev/config.toml` 加载配置；若文件不存在，则返回一个包含 `claude` 和 `codex` 的硬编码默认配置。
    3.  在项目文档中提供 `config.toml` 的配置示例。

*   **Step 2: 状态管理扩展 (Task 2)**
    1.  修改 `src/state.rs` 中的 `WorktreeDetails` 结构体，增加一个 `task_id: String` 字段，并使用 `#[serde(default)]` 确保向后兼容。
    2.  在状态加载逻辑中，对旧数据进行迁移：遍历所有 worktree 详情，如果其 `task_id` 为空，则用其自身的名称作为 `task_id` 的默认值。

*   **Step 3: 实现 `agentdev start` 命令 (Task 3)**
    1.  使用 `clap` 定义 `start` 子命令，包含 `prompt`、可选的 `--agents` 和 `--name` 参数。
    2.  实现命令逻辑：加载配置，确定任务名（用户指定或随机生成），然后循环遍历所有目标 Agent，为每个 Agent 执行 `git worktree add` 和 `tmux new-session -d` / `tmux send-keys`，最后将所有新创建的 worktree 信息（包含正确的 `task_id`）写入状态文件。

*   **Step 4 & 5: Dashboard 改造与分层 Diff (Task 4 & 5)**
    1.  **数据结构**: 定义一个 `enum ListItem`，包含 `Task(TaskGroup)` 和 `Worktree(&WorktreeDetails)` 两个变体，用于表示 TUI 列表中的不同层级。
    2.  **数据准备**: 在渲染前，将扁平的状态数据按 `task_id` 重组为 `Vec<TaskGroup>`。
    3.  **UI 渲染**: 修改 `ratatui` 的渲染循环，以树状结构绘制列表。根据任务节点的 `is_expanded` 状态决定是否渲染其下的 Agent 子节点。
    4.  **核心预览逻辑**: 在 TUI 的事件处理循环中：
        *   获取当前高亮的 `ListItem`。
        *   **如果选中的是 `Task`**: 遍历其下的所有 worktree，分别执行 `git diff`，然后将所有 `diff` 输出（每个都带有 Agent 头部标识）合并成一个字符串，显示在预览窗格。
        *   **如果选中的是 `Worktree`**: 只对这一个 worktree 执行 `git diff`，并将其输出显示在预览窗格。

*   **Step 6: 实现 `agentdev delete-task` 命令 (Task 6)**
    1.  使用 `clap` 定义 `delete-task` 子命令，接受一个 `task_name` 参数。
    2.  实现命令逻辑：根据 `task_name` 从状态文件中筛选出所有相关的 worktree，向用户展示并请求确认，然后依次执行 `tmux kill-session` 和 `git worktree remove`，最后清理状态文件。

**测试计划**
*   **单元测试**: 覆盖配置加载和状态迁移的边缘情况。
*   **集成测试**: 编写脚本测试 `agentdev start` -> `agentdev delete-task` 的完整生命周期，验证文件系统和 `tmux` 状态的正确性。
*   **手动测试**: 重点测试 Dashboard 的交互，包括任务折叠/展开、分层 `diff` 视图的正确切换，以及 `Enter` 键附着 `tmux` 会话的功能。
