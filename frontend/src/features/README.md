# Feature Modules

前端功能域的正式位置：

- `agent`：目标、权限确认、上下文和工具执行视图。
- `chat`：消息列表与输入编排。
- `workspace`：文件树、Git 分支和 Diff。
- `workflow`、`voice`、`media`、`companion`、`live2d`：独立能力面板。
- `settings`：配置编辑用例。
- `pack`：角色包编辑器。

每个功能域只创建实际需要的组件、Hook、模型和服务，不机械生成空层。
跨功能共享的无状态视图进入 `shared/components`；稳定 IPC 契约经 `lib/api`
访问，功能组件不得直接调用 Tauri `invoke`。
