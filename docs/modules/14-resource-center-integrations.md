# 14. 外部资源中心：Skill、会话与使用量

资源中心是 Desktop 层的受限适配缝，位于确定性 Agent 主循环之外。它借鉴 cc-switch 的“发现—选择—导入—管理”分离，但不把外部工具的配置或会话文件变成 Demiurge 的执行真相。

## Skill

资源中心只读扫描以下来源：

- `~/.codex/skills`、`~/.claude/skills`、`~/.agents/skills`
- 当前项目的 `.codex/skills`、`.claude/skills`、`.agents/skills`
- Demiurge 自己的 `data_dir/skills`

导入时复制到 `data_dir/skills/<safe-directory>`，写入 `skill_registry.json` 记录来源、路径、时间和启用状态。导入不会执行 `SKILL.md`，frontmatter 中的 tools/permissions 只是描述，不授予权限。来源冲突默认使用稳定后缀或拒绝覆盖；复制过程限制文件数量、总大小并跳过符号链接。

现有 `agent/skills.rs` 继续负责确定性选择、Pack policy 和上下文大小限制；资源中心不向 prompt 构建路径发起网络请求。

市场搜索使用 skills.sh，安装通过 GitHub zip 到临时目录，再按本地 Skill 导入规则校验。下载失败、压缩包路径越界或缺少 `SKILL.md` 都直接拒绝。

## Codex / Claude Code 会话

外部会话默认只读：

- Claude Code：`~/.claude/projects/**/*.jsonl`
- Codex：`~/.codex/sessions/**/*.jsonl` 与 `archived_sessions/**/*.jsonl`

扫描只生成轻量元数据；预览/导入才加载消息。导入会把经过 provider 适配的消息复制成新的 Demiurge `Session`，原文件不修改、不删除，也不把外部 `cwd` 绑定为 Demiurge workspace。源路径必须 canonicalize 后位于对应 provider 根目录内，超过大小上限或包含坏 JSON 行时整次导入拒绝。

Codex 和 Claude 的配置只发现安全偏好字段（如 model、sandbox_mode、reasoning_effort）；API key、OAuth、MCP secret、hooks 和权限字段永不复制或写入会话。

### Demiurge 会话归档

本地会话增加 `archived` / `archived_at` 标记。归档不会删除消息、事件或项目路径，侧栏按“未归档 → 最近更新时间”显示，并把已归档会话集中到“已归档 · 按最近更新时间”分组；项目名仍作为副标题展示，便于按项目识别。恢复只是清除标记，因而不会破坏 append-only 事件源。归档和删除是两条不同操作：归档可恢复，删除仍受正在运行回合保护。

## 使用量

模型请求在成功或失败后向 `data_dir/usage.jsonl` 追加一条记录，包含 provider、model、purpose、session、输入/输出/总 token、延迟和状态。主 Agent、子 Agent 与 `/dream` 都记录；统计命令从 JSONL 重放并按 provider、model、日期聚合。

provider 返回精确 usage 时使用精确值，否则只记录 fallback 估算 token 数。LLM 适配层同时保留 provider 报告的 cached input/cache creation 字段；OpenRouter 模型目录刷新后把 `context_length` 与 prompt/completion/cache 单价保存到 `model_catalog.json`，新请求按快照计算本地 USD 估算，并明确区分已定价/未定价请求。它不是 provider 账单，也不会对无目录模型猜价。

OpenRouter 目录通过官方 `GET /api/v1/models` 以 text + tools + newest 条件刷新，设置页会显示缓存时间；网络失败不会清空现有目录或阻断本地运行。静态列表仅作为未刷新时的兜底。

这条旁路不改变 `runner` 的 stop/工具决策，不把市场、外部会话或统计反馈注入确定性主循环。
