# 审查与交接规范

报告完成前，逐项检查本文件。

## 审查清单

### 任务范围

- 修改符合用户要求。
- 保留工作区中与任务无关的已有修改。
- 未执行用户没有要求的提交、推送、历史改写、发布或部署。

### 模块归属

- 每个新文件都只有一个清晰的责任归属。
- Controller 和运行时入口保持轻量。
- 依赖指向 Common、Core、Framework，不反向指向 Desktop。
- 未增加空层或推测性抽象。
- 新接口隐藏了有意义的复杂度。

### 兼容性

- 除非任务明确要求迁移，否则 IPC 命令名、事件名和序列化字段名保持不变。
- 设置默认值和持久化 JSON 保持兼容。
- 安全检查、恢复路径和错误语义保持有效。

### 验证证据

- 相关测试覆盖了修改行为。
- 更大范围的测试覆盖了集成风险。
- 需要时已经执行格式检查、类型检查和构建。
- `git diff --check` 通过。
- 构建产物和缓存没有被意外纳入版本管理。

## 最终交接格式

先说结果，再按以下结构交接：

```text
结果
- 现在可以做什么，或者发生了什么变化。

代码归属与调用链
- frontend feature -> IPC -> controller -> biz -> core/framework

修改区域
- path：职责和放在这里的原因

兼容性
- 有意保持或迁移的契约

验证
- 实际命令：结果

复查说明
- 风险、警告、假设和未验证范围
- 当前分支、是否提交、是否推送
```

运行环境支持时，文件路径使用可点击链接。

## Commit 和 Pull Request 规范

只有获得用户授权时才能提交或推送。

推荐提交信息：

```text
feat(workspace): add generation-safe file preview
fix(session): preserve active workspace on failed navigation
refactor(architecture): separate Tauri controller and biz
test(permission): cover captured session identity
docs(architecture): record WebDAV adapter seam
```

一个 Commit 只表达一个完整的修改原因。不要把架构移动、依赖升级和无关格式化混在同一次提交中。

Pull Request 必须包含：

- 问题和预期结果。
- 架构归属及依赖变化。
- 用户可见影响和兼容性影响。
- 测试证据。
- 迁移和回滚方法。
- 安全或数据处理影响。

## 作者与责任人

- Git Commit 元数据记录谁编写或修改了代码。
- IDE Code Vision 和 Git blame 自动展示贡献者。
- `CODEOWNERS` 记录审查责任，不表示作者身份。
- 结对开发或 AI 辅助可以按照团队约定在 PR 或 Commit Trailer 中披露。
- 禁止伪造其他人的作者身份。
