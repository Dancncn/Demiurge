# Demiurge 架构与模块归属

决定代码放置位置前，必须阅读本文件。

## 仓库目录职责

| 位置 | 应该负责 | 禁止负责 |
|---|---|---|
| `frontend/src/app` | 应用组合、导航外壳、全局订阅 | Provider 实现、后端持久化模型 |
| `frontend/src/features/<feature>` | 功能界面、Hook、局部状态、功能专用辅助代码 | 其他功能的内部实现、直接持久化 |
| `frontend/src/shared/components` | 无状态、可复用的界面组件 | 业务编排、直接调用 Tauri |
| `frontend/src/lib` | 类型化 IPC 门面、共享契约、跨功能运行时辅助能力 | 单一功能界面、Provider 密钥 |
| `backend/Demiurge-common` | 稳定的共享序列化契约 | Tauri、文件系统、HTTP、用例编排 |
| `backend/Demiurge-core` | 不依赖 Tauri 的领域状态、约束和规则 | 具体持久化、远程 Provider、窗口管理 |
| `backend/Demiurge-framework` | 持久化和外部系统适配器 | Tauri 命令、桌面端用例 |
| `backend/Demiurge-desktop/src/controller` | Tauri 命令参数、校验、响应适配 | 状态锁、持久化、文件系统、HTTP SDK |
| `backend/Demiurge-desktop/src/biz` | 单个用户用例和跨模块编排 | Tauri 命令属性、供应商协议细节 |
| `backend/Demiurge-desktop/src/starter.rs` | 运行时装配、恢复、窗口、命令注册 | 领域决策 |
| `assets` | 项目源图片和品牌素材 | 运行时代码 |
| `resources` | 纳入版本管理的运行时种子数据和默认 Pack | 构建生成物 |
| `docs` | 架构、行为和运维文档 | 自动生成物 |

## 后端功能调用链

选择能够隐藏实现细节的最短调用链：

```text
Controller ReqDto
  -> Biz 输入 / 用例
  -> Core 规则或 Desktop 领域模块
  -> Framework Repository / Remote 适配器
  -> 显式结果转换
  -> Controller ResDto
```

不是每项功能都必须拥有每一层：

- 只有 IPC 表示与内部表示不同时才增加 DTO。
- 只有某个领域表示确实承载规则或约束时才增加 BO。
- 只有持久化表示与领域表示存在实际差异时才增加 DO。
- 只有存在真实查询语义时才增加 Query。
- 只有持久化需要替换，或需要隐藏事务、恢复等实质行为时，才增加 Repository 接口。
- 接入外部协议或可替换 Provider 时才增加 Remote 边界。

## 前端功能调用链

```text
main.tsx
  -> app shell
  -> feature component/hook
  -> typed lib/api facade
  -> Tauri IPC
```

Feature 之间只能通过有意设计的共享接口互相引用。只有工具确实拥有多个使用方时，才将其移动到 `lib`；只有视觉组件确实拥有多个使用方时，才将其移动到 `shared`。

## 依赖审查问题

接受一个新的 import 前，逐项确认：

1. 引用方是否知道了超出自身职责范围的细节？
2. 下层模块是否反向引用了入口层或编排层？
3. 替换 Provider 或持久化格式时，是否仍需要修改大量调用方？
4. 新接口是否明显小于它所隐藏的实现复杂度？
5. 测试能否通过与生产代码相同的接口验证行为？

## 扩展策略

未来能力写入架构文档，不通过空源码目录、占位类、未使用 Trait 或假适配器来预留：

- 项目尚未消费或发布消息时，不创建 MQ 模块。
- 持久化仍是本地 JSON 时，不创建数据库 DAO 层。
- 尚无独立后台管理运行时，不创建 Boss/Admin 应用。
- 尚未接入具体供应商时，不创建该供应商的适配器。
