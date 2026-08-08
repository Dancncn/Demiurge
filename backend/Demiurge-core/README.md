# Core

不依赖 Tauri 和基础设施的核心规则 crate。目前承载 Session Store 与 Goal
状态机；它们保留原有 JSON 字段和默认语义。

每个模块应通过小而稳定的接口隐藏复杂实现。核心代码不能依赖 Tauri、具体文件格式、操作系统窗口或某个 LLM Provider。

只有能真正脱离桌面运行时的领域规则才进入这里。依赖事件发送、窗口或 Tool
运行态的逻辑留在 Desktop Service，避免制造无法兑现的纯领域抽象。
