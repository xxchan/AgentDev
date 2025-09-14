# agentdev（原 xlaude）- Claude 实例管理工具

本工具已从 xlaude 更名为 agentdev：
- 可执行文件名：`agentdev`
- worktree 相关命令已收敛为二级子命令 `worktree`（别名 `wt`）
- 兼容性：原顶层命令（create/open/delete/…）仍可用，便于平滑迁移

示例映射：
- `agentdev worktree create feature-x`（原 `xlaude create feature-x`）
- `agentdev worktree open feature-x`（原 `xlaude open feature-x`）
- `agentdev dashboard`（原 `xlaude dashboard`）

下文旧文档中的 `xlaude …` 用法在过渡期仍有效，推荐逐步迁移到 `agentdev worktree …`。

## 核心功能

### xlaude create [name]
创建新的 worktree 和分支：
- 必须在 main/master/develop 分支上执行
- 如果不提供 name，自动从 BIP39 词库随机选择一个词
- 创建新分支 `<name>`
- 创建 worktree 到 `../<repo-name>-<name>` 目录
- **不会自动启动 Claude**

### xlaude open [name]
打开已存在的 worktree 并启动全局配置的 Agent（默认为 Claude）：
- 有参数：打开指定的 worktree
- 无参数：
  - 如果当前目录是 worktree（非 main/master/develop）：直接打开当前 worktree
  - 如果当前 worktree 未被管理：询问是否添加并打开
  - 否则：显示交互式选择列表
- 切换到 worktree 目录
- 启动全局配置的 `agent` 命令（默认：`claude --dangerously-skip-permissions`）
- 继承所有环境变量

### xlaude delete [name]
删除 worktree 并清理：
- 有参数：删除指定的 worktree
- 无参数：删除当前所在的 worktree
- 检查未提交的修改和未推送的 commit
- 检查分支是否已完全合并，未合并时询问是否强制删除
- 需要时进行二次确认
- 自动删除 worktree 和本地分支（如果安全）

### xlaude add [name]
将当前 worktree 添加到 xlaude 管理：
- 必须在 git worktree 中执行
- 如果不提供 name，默认使用当前分支名
- 检查是否已被管理，避免重复添加
- 适用于手动创建的 worktree 或从其他地方克隆的项目

### xlaude list
列出所有活跃的 worktree，显示：
- 名称
- 仓库名
- 路径
- 创建时间
- Claude sessions（如果存在）
  - 显示最多 3 个最近的 session
  - 每个 session 显示：最后更新时间和最后的用户消息
  - 超过 3 个时显示剩余数量

### xlaude clean
清理无效的 worktree：
- 检查所有管理的 worktree 是否仍存在于 git 中
- 自动移除已被手动删除的 worktree
- 适用于使用 `git worktree remove` 后的清理
- 保持 xlaude 状态与 git 状态同步

### xlaude rename <old_name> <new_name>
重命名 worktree 状态：
- 重命名 xlaude 管理中的 worktree 名称
- 仅更新 xlaude 状态，不影响实际的 git worktree 或目录
- 检查新名称是否已存在，避免冲突
- 保留所有 Claude sessions 和元数据

### xlaude dir [name]
获取 worktree 的目录路径：
- 有参数：返回指定 worktree 的绝对路径
- 无参数：显示交互式选择列表
- 输出纯路径，无装饰符，便于 shell 命令使用
- 适用于与其他工具集成（cd、编辑器、zoxide 等）

## 技术实现

- 使用 Rust 开发
- 直接调用系统 git 命令
- 状态持久化位置：
  - macOS: `~/Library/Application Support/com.xuanwo.xlaude/state.json`
  - Linux: `~/.config/xlaude/state.json`
  - Windows: `%APPDATA%\xuanwo\xlaude\config\state.json`
  - Worktree key 格式：`<repo-name>/<worktree-name>`（v0.3+）
  - 自动迁移旧版本格式到新格式
- 使用 clap 构建 CLI
- 使用 BIP39 词库生成随机名称
- 彩色输出和交互式确认
- 集成测试覆盖所有核心功能

## 全局 Agent 配置

`agent` 字段用于配置启动会话时使用的完整命令行（全局唯一，对 `open` 和 Dashboard 均生效）。

- 若未设置，默认值为 `claude --dangerously-skip-permissions`。
- 配置示例与参考请直接查看首次运行自动生成的 `~/.config/agentdev/config.toml`，无需在文档中阅读示例格式。

## 使用示例

```bash
# 在 opendal 项目中创建新的工作分支
cd opendal
xlaude create feature-x  # 创建 ../opendal-feature-x 目录

# 使用随机名称创建
xlaude create  # 可能创建 ../opendal-dolphin 目录

# 打开并启动 Claude
xlaude open feature-x  # 打开指定的 worktree
xlaude open  # 如果在 worktree 中直接打开，否则交互式选择

# 将已存在的 worktree 添加到管理
cd ../opendal-bugfix
xlaude add  # 使用当前分支名作为名称
xlaude add hotfix  # 或指定自定义名称

# 列出所有活跃的实例
xlaude list

# 删除当前 worktree
xlaude delete

# 删除指定 worktree
xlaude delete feature-x

# 清理无效的 worktree
xlaude clean

# 重命名 worktree
xlaude rename feature-x feature-improved

# 典型工作流
xlaude create my-feature  # 创建 worktree
xlaude open my-feature   # 打开并开始工作
# ... 工作完成后 ...
xlaude delete my-feature # 清理 worktree

# 直接在当前 worktree 中启动
cd ../opendal-feature
xlaude open  # 自动检测并打开当前 worktree

# 获取 worktree 路径（用于目录切换）
cd $(xlaude dir feature-x)  # 切换到指定 worktree
xlaude dir  # 交互式选择 worktree 并输出路径

# 配合 shell function 使用
# 在 .bashrc/.zshrc 中添加：
# xcd() { cd $(xlaude dir "$@"); }
xcd feature-x  # 快速切换到 worktree

# 与其他工具集成
code $(xlaude dir feature-x)  # 用 VSCode 打开
vim $(xlaude dir feature-x)/src/main.rs  # 编辑文件
```
