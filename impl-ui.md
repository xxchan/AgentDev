### **`agentdev ui`：项目需求、任务与实施计划**

> **2025-10 Update:** The legacy task/agent workflow described here has been deprecated. The dashboard now focuses solely on worktree management; the notes below are retained for historical context.

#### **第一部分：最终需求 (The "What")**

**项目目标**
创建一个名为 `agentdev ui` 的 Web 应用，为 `agentdev` 工具提供一个直观、易用的图形化界面。此 UI 旨在彻底摆脱对 `tmux` 知识的依赖，让任何用户都能方便地对多个命令行 AI Agent 在同一任务上的表现进行并排的“体感对比 (vibe eval)”。

**核心工作流**
1.  **启动 (Launch)**: 用户在终端中运行 `agentdev ui`。该命令会启动一个本地 Web 服务器，并自动在浏览器中打开 `http://localhost:xxxx`，展现 `agentdev` 的图形化主界面。
2.  **任务概览 (Overview)**: UI 主界面采用经典的三栏式布局：
    *   **左侧边栏**: 以可折叠的树状结构展示所有评测任务 (`Task`) 及其包含的 Agent 实例。提供一个醒目的“新建任务”按钮。
    *   **主编辑区**: 根据用户的选择，动态展示 `git diff` 视图。
    *   **底部面板**: 提供一个嵌入式的终端，用于与选定的 Agent 进行实时交互。
3.  **创建任务 (Create)**: 用户点击“新建任务”按钮，在弹出的模态框中输入初始指令 (`prompt`)。后端会为配置中的每个 Agent 创建独立的 `git worktree` 和 `tmux` 会话，并在左侧边栏实时更新任务列表。
4.  **评估对比 (Evaluate & Compare)**:
    *   **宏观对比**: 当用户在左侧边栏点击一个**任务节点**时，主编辑区会显示一个**合并后**的 `diff` 视图，将该任务下所有 Agent 产生的代码变更并排展示，方便快速横向比较。
    *   **微观审查**: 当用户点击一个具体的 **Agent 节点**时，主编辑区仅显示**这一个 Agent** 的 `git diff`，方便深入审查其方案。
5.  **深度交互 (Engage)**: 当用户在左侧边栏选中一个 Agent 并点击“连接”后，底部面板的嵌入式终端将激活，实时“投射”出该 Agent 所在的 `tmux` 会话内容。用户可以直接在该终端中输入命令，与 Agent 进行完整的、原生的交互。
6.  **清理 (Clean Up)**: 用户可以通过 UI 上的按钮或菜单项，一键删除某个评测任务，后端将自动清理所有相关的 `git worktree` 和 `tmux` 会话。

**技术架构**
*   **统一二进制**: 前端 (Next.js/React) 的静态资源（HTML, CSS, JS）将被编译并**嵌入**到后端 Rust 二进制文件中。
*   **后端 (Rust)**: 使用 `axum` 框架。它既负责提供 API 和 WebSocket 服务，也负责托管前端静态文件。
*   **核心引擎**: 后端保留并封装对 `git` 和 `tmux` 的调用，作为实现 Agent 并发运行和交互的核心引擎。

---

#### **第二部分：高层任务分解 (The "How" - Abstract)**

为了实现上述需求，我们将项目分解为后端和前端两大模块，共计八个核心任务。

**后端 (Rust)**
1.  **Task B1: 静态文件嵌入与服务**: 使用 `rust-embed` 或类似库将前端编译产物打包进二进制文件，并配置 `axum` 来提供这些文件。
2.  **Task B2: API 与 WebSocket 路由**: 设计并实现 `axum` 的路由，包括用于数据查询的 RESTful API 和用于实时交互的 WebSocket 端点。
3.  **Task B3: `tmux` 交互服务**: 编写服务逻辑，通过 `tmux` 命令实现捕获窗格内容、发送按键等功能，并通过 WebSocket 与前端通信。

**前端 (Next.js/React)**
4.  **Task F1: 项目初始化与布局**: 创建 Next.js 项目，集成 `Tailwind CSS`，并搭建三栏式 UI 骨架。
5.  **Task F2: 左侧任务树实现**: 开发 `TaskTree` 组件，负责从后端获取数据、渲染树状列表，并处理用户选择事件。
6.  **Task F3: `diff` 视图实现**: 开发 `GitDiffViewer` 组件，集成 `@git-diff-view/react` 库，直接渲染来自后端的 git patch。
7.  **Task F4: 嵌入式终端实现**: 开发 `TmuxTerminal` 组件，集成 `xterm.js`，并实现与后端 WebSocket 的双向通信。
8.  **Task F5: 客户端逻辑与状态管理**: 开发自定义 Hooks (`useTasks`, `useAgentDevSocket`) 来封装 API 调用和 WebSocket 通信，管理客户端状态。

---

#### **第三部分：详细实施计划 (The "How" - Concrete)**

**环境设置**
1.  在 `agentdev` 项目中创建一个 `monorepo` 结构，包含 `apps/backend` 和 `apps/frontend` 两个目录。
2.  配置根 `package.json` 以便能同时管理前后端项目的依赖和启动脚本。

**编码步骤**

*   **Step 1: 后端 - 静态文件嵌入 (Task B1)**
    1.  在 `apps/backend/Cargo.toml` 中添加 `axum-embed` 和 `rust-embed` 依赖。
    2.  在 `apps/backend/src/main.rs` 中，定义一个 `struct` 来嵌入前端 `build` 目录的产物。
        ```rust
        #[derive(RustEmbed)]
        #[folder = "../../frontend/out"] // 指向 Next.js 静态导出目录
        struct FrontendAssets;
        ```
    3.  配置 `axum` 路由，使用 `axum_embed::Serve<FrontendAssets>` 来服务静态文件。设置一个 `fallback` 路由，将所有未匹配的路径都指向 `index.html`，以支持前端路由。

*   **Step 2: 后端 - API 与 WebSocket (Task B2 & B3)**
    1.  在 `apps/backend/src/routes/` 中定义 API 路由：
        *   `GET /api/tasks`: 返回所有任务及其下的 Agent 列表。
        *   `POST /api/tasks`: 接收 `prompt`，创建新任务，返回新任务信息。
        *   `DELETE /api/tasks/{task_id}`: 删除指定任务。
        *   `GET /api/tasks/{task_id}/agents/{agent_id}/diff`: 返回指定 Agent 的 `git diff` 原始文本。
    2.  定义 WebSocket 路由 `GET /ws/tasks/{task_id}/agents/{agent_id}/attach`。
    3.  实现 WebSocket 的 `handler` 函数：
        *   **连接建立**: 启动一个循环任务（如每 200ms）。
        *   **循环任务**: 调用 `tmux` 服务，执行 `tmux capture-pane -p -t <session>` 捕获窗格内容，并通过 WebSocket 发送给客户端。
        *   **接收消息**: 当从客户端收到消息（用户输入）时，调用 `tmux` 服务，执行 `tmux send-keys -t <session> '<input>'`。

*   **Step 3: 前端 - 初始化与布局 (Task F1)**
    1.  在 `apps/frontend` 目录中，使用 `create-next-app` 创建项目，并配置为**静态导出模式** (`output: 'export'` in `next.config.js`)。
    2.  集成 `Tailwind CSS`，并参考专业设计，在 `tailwind.config.ts` 和 `globals.css` 中定义好颜色、字体等主题变量。
    3.  创建 `components/layout/MainLayout.tsx`，使用 Flexbox 或 CSS Grid 实现三栏布局的骨架。

*   **Step 4: 前端 - 核心组件开发 (Task F2, F3, F4)**
    1.  **`TaskTree.tsx`**:
        *   使用 `useEffect` 在组件挂载时调用 `fetch('/api/tasks')` 获取任务列表。
        *   使用 `useState` 管理任务列表数据和当前选中的条目 ID。
        *   将数据渲染成一个嵌套的 `<ul>`/`<li>` 列表，并处理点击事件来更新选中状态。
    2.  **`GitDiffViewer.tsx`**:
        *   接收 `diffText: string` 作为 prop。
        *   使用 `@git-diff-view/react` 提供的 `DiffView` 组件渲染统一或并排的 diff 视图，并提供模式切换、换行和复制补丁等操作。
    3.  **`TmuxTerminal.tsx`**:
        *   集成 `xterm.js` 和 `xterm-addon-fit` (用于自适应容器大小)。
        *   在组件挂载时初始化 `Terminal` 实例。
        *   提供一个 `connect(url)` 方法，该方法内部会创建 `WebSocket` 实例，并设置好 `onmessage` 和 `onopen` 等回调。
        *   实现 `terminal.onData(data => ws.send(data))` 将用户输入转发到后端。

*   **Step 5: 前端 - 逻辑与状态管理 (Task F5)**
    1.  创建 `hooks/useTasks.ts`：
        *   封装获取、创建、删除任务的 `fetch` 调用。
        *   暴露 `tasks`, `isLoading`, `error`, `createTask`, `deleteTask` 等状态和方法。
    2.  创建 `hooks/useAgentDevSocket.ts`：
        *   封装 `WebSocket` 的完整生命周期管理（连接、断开、消息收发、自动重连）。
        *   暴露 `isConnected`, `lastMessage`, `sendMessage` 等状态和方法。
    3.  在顶层页面组件 (`app/page.tsx`) 中使用这些 Hooks，并将状态和方法通过 props 或 React Context 传递给子组件。

**测试计划**
1.  **后端单元测试**: 针对 `git` 和 `tmux` 服务的封装进行测试。
2.  **前后端集成测试**:
    *   **启动流程**: 运行 `cargo run --bin agentdev -- ui`，验证浏览器能成功打开并加载 UI。
    *   **核心工作流**: 手动测试“创建任务 -> 查看合并 Diff -> 查看单个 Diff -> 连接终端交互 -> 删除任务”的完整流程。
    *   **网络鲁棒性**: 在 UI 运行时，手动 `kill` 后端进程，验证前端 WebSocket 是否尝试重连；重启后端后，验证连接是否能自动恢复。
3.  **UI/UX 测试**: 检查 UI 在不同屏幕尺寸下的响应式表现，验证所有按钮、动画和状态转换是否流畅、符合预期。
