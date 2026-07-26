# Common

可编译的共享契约 crate。目前包含 Conversation 消息模型、Settings/MCP
配置模型及兼容默认值；Desktop 通过兼容 facade 继续保持原序列化语义。

禁止放入：

- 仅被一个模块使用的类型。
- 业务编排。
- Tauri、文件系统、HTTP 或 Provider 实现。
- 无明确调用方的“未来可能用到”工具。

新增类型必须有多个真实调用方，不能把 Common 变成工具垃圾桶。
