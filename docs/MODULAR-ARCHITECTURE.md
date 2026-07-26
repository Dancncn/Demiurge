# Demiurge 模块化架构

## 状态

本文记录已落地的模块化单体架构和长期约束。

根目录已经完成物理分离：

```text
Demiurge/
├─ frontend/
├─ backend/
│  ├─ Demiurge-common/
│  ├─ Demiurge-core/
│  ├─ Demiurge-framework/
│  └─ Demiurge-desktop/
├─ assets/
├─ resources/
├─ docs/
├─ scripts/
└─ tests/
```

后端四个目录均为真实 Cargo crate。Desktop 的全部 117 个 command 遵循统一调用链：

```text
starter
  ↓ 注册
controller::<feature>
  ↓ IPC 参数/响应适配
biz::<feature>
  ↓ 编排
core + framework + desktop service/adapter
```

`desktop/src/lib.rs` 只保留模块声明与启动 facade。架构测试禁止 command、
持久化和远程访问重新回流到入口层。

## 模块职责

### frontend

React 用户界面、交互状态和 Tauri invoke/listen Adapter。前端只依赖稳定 IPC 契约，不了解后端持久化对象和第三方 Provider 实现。

### common

少量稳定、被多个模块实际共享的契约。禁止成为通用工具垃圾桶。

已落地内容：

- Conversation 消息契约。
- Settings 与 MCP 共享配置契约。
- 多个模块共同读取的稳定枚举和值对象。

### core

可脱离桌面运行时独立测试的业务规则。目前包含 Session 和 Goal 状态。

`core` 不得依赖：

- Tauri。
- 具体文件路径或 JSON 布局。
- reqwest、Keyring、Git 命令和操作系统窗口。
- OpenAI、Gemini、DashScope 等具体 Provider。

### framework

位于核心接缝处的 Adapter：

- `persistence`：Session、Settings、Memory、Permission 持久化。
- `remote`：目前已迁移 WebDAV；其余 Provider 在形成稳定接缝前仍是 Desktop
  专属 Adapter。
- `system`：Shell、文件系统、Git、窗口、截图、OCR、剪贴板。
- `security`：凭据存储、URL 安全和敏感数据处理。

### desktop

Tauri 桌面入口和运行时装配：

- `controller`：Tauri commands、参数校验和 DTO 转换。
- `biz`：Agent、Session、Goal、Memory、Pack、Permission、Remote、Window
  等桌面用例编排。
- `starter`：`AppState`、窗口、命令注册和 Adapter 装配。

Controller 必须保持轻量，不能直接访问持久化实现、操作系统命令或第三方 SDK。

## 依赖方向

```text
frontend
    │ Tauri IPC
    ▼
desktop controller
    ▼
desktop biz
    ▼
core
    ▲
    │ implements core interfaces
framework adapters

desktop starter ──> core + framework + controller
```

允许：

```text
desktop -> core
desktop -> framework
framework -> core
core -> common
framework -> common
desktop -> common
```

禁止：

```text
core -> framework
core -> desktop
framework -> desktop
common -> core/framework/desktop
controller -> persistence DAO
controller -> concrete remote Provider
```

## 功能内部转换

以 Session 为例：

```text
RenameSessionReqDTO
    ↓ controller
RenameSessionInput / SessionBO
    ↓ biz
SessionService
    ↓ SessionRepository interface
FileSessionRepository adapter
    ↓ converter
SessionDO
```

- DTO 只存在于入口契约。
- BO 表达业务状态和规则。
- DO 表达磁盘或数据库持久化格式。
- Query 表达持久化查询条件。
- DTO、BO、DO 不得互相复用来省略转换。
- 没有独立持久化语义的功能不创建 DO、Query 或 DAO。

## 深模块要求

目录数量不是架构质量。每个模块必须通过较小接口隐藏足够多的复杂实现：

- Agent 对外暴露“执行一轮”的接口，不让 Controller 了解流式解析、权限、Tool loop 和持久化细节。
- Provider 通过统一完成接口隐藏不同供应商的请求和 SSE 方言。
- Permission 通过一次裁决接口隐藏规则查找、作用域、确认和审计。
- Session Repository 隐藏原子写入、备份恢复和磁盘布局。

删除一个只有参数透传作用的层，如果复杂度没有扩散到多个调用方，说明该层不值得存在。

## 预留扩展规则

- 未来位置通过 README 或 ADR 说明，不创建无行为的空类。
- 只有存在真实变化时才增加接口和 Adapter。
- 一个实现通常不足以证明需要抽象；两个实现或明确替换需求才形成真实接缝。
- 新 Provider 必须实现既有接口并通过契约测试。
- 新功能不能通过反向依赖绕过现有层。

## 已完成批次

### 第一批：物理目录分离

- 移动前端、后端、资源和测试。
- 建立 npm workspace 和 Cargo workspace。
- 修复 Vite、Tauri、测试和脚本路径。
- 不修改 IPC、事件、配置或持久化语义。

### 第二批：Desktop Controller/Biz/Starter

- `AppState` 独立位于 `starter/state.rs`。
- 117 个 command 全部迁入 18 个功能 Controller。
- 复杂实现下沉至对应 Biz；Controller 不持锁、不持久化、不访问远程 SDK。
- 窗口生命周期与原生窗口调用集中在 `biz/window.rs`。

### 第三批：Common/Core/Framework

- Common、Core、Framework 成为可编译 crate。
- Conversation、Settings、MCP 契约进入 Common。
- Session、Goal 状态进入 Core。
- 原子文件、Session/Settings 持久化与 WebDAV 进入 Framework。

### 第四批：前端功能域

- 主应用和导航进入 `src/app`。
- 业务视图迁入 agent、chat、workspace、workflow、voice、companion、
  live2d、media、pack、settings 功能域。
- 无业务状态的组件进入 `src/shared/components`。
- 旧 `src/components` 聚合目录被移除，导入统一使用 `@/`。

每次后续变更必须保持前端测试与构建、Rust 格式、架构边界测试和 Workspace
测试通过。新能力只在真实存在时增加模块；没有 MQ、数据库或后台管理端时不创建
对应空模块。
