# 模型档位、作用域子 Agent 与故障转移

## 决策

Demiurge 保留确定性的主循环：一次主回合仍然由 runner 组装上下文、调用 LLM、处理工具调用并追加会话消息。模型调度是 LLM seam 上的适配器，不创建第二套 stop/continue 拓扑。

设置中的 model_routing 是一个组合配置层：

- haiku：快速、低成本的阅读、检索和验证 worker。
- sonnet：均衡的文档整理和中等复杂度 worker。
- opus：规划、架构判断等强推理 worker。
- 每个档位可绑定当前提供商的模型 ID；留空表示沿用当前模型。
- Anthropic 提供商在没有显式绑定时使用其对应的官方档位模型。其他提供商不猜测模型 ID，避免把价格、能力和凭证静默混在一起。

子 Agent 获得独立的 brief/recent/fork 上下文，结果以工具结果返回主 Agent，不追加到主会话的对话投影。每个子 Agent 还可以绑定一个作用域：

- read_only：读取文件、搜索、查看 Git 状态和网页。
- docs_write：在显式授权的受控槽内使用文档编辑工具，但目标只能是 docs/** 或根目录的 Markdown / TXT / RST 文档；不能写源码、执行 shell、截图或派生子 Agent。

作用域在提示词、工具 schema 和执行器三处同时表达，并在执行器重复校验路径。提示词不是安全边界。

## 故障转移

model_routing 的备用模型是有序列表，只在同一提供商的已配置模型 ID 中选择。每次真实请求都先追加完整模型可见消息和工具 schema 的审计事件，然后才进入 provider adapter；故障转移重试也会追加自己的事件。

路由采用有限重试和进程内断路器：

- 只对 429、5xx、529、超时、连接中断、过载和临时容量错误重试。
- 认证、权限、参数、取消和模型配置错误直接返回。
- 连续失败达到阈值后暂时打开断路器；冷却后允许一次半开探测。
- 所有候选都被打开时，只探测主模型一次，不推测新的模型或提供商。
- 流式失败切换前重置当前 UI 投影，避免把失败候选已经吐出的半截文本与新候选拼接。

这吸收了 cc-switch 的队列和断路器思想，但不复制它的本地代理、Base URL 改写或跨应用接管。Demiurge 当前设置只有一个活动提供商凭证，跨提供商路由必须等凭证 profile 成为正式配置后再做。

## 整流器取舍

cc-switch 的 thinking/media rectifier 是配置门控、错误特定、有限次数的请求适配修复。Demiurge 已在 llm::ProviderProfile 和各 provider adapter 内完成 reasoning、finish reason、工具 schema 和 usage 的规范化，因此不新增一个会偷偷改写任意请求的通用代理整流器。

当前采用的整流原则是：

1. 先在 provider adapter 内做协议规范化；
2. 只把可判定的暂时性错误交给 model route failover；
3. 任何未知错误 fail closed；
4. 只对 UI 流投影做重置，不修改主循环的决策拓扑。

## 与 Skill / 会话资源中心的关系

Skill 导入、来源发现、启停管理、Codex / Claude Code 配置与会话导入，以及归档、用量和模型目录管理，已经由资源与会话设置页承载。自定义 .demiurge/agents/*.json 现在可以增加：

~~~json
{
  "model_tier": "haiku",
  "scope": "read_only"
}
~~~

文档 Agent 可以使用：

~~~json
{
  "name": "documenter",
  "model_tier": "sonnet",
  "scope": "docs_write",
  "allowed_tools": ["read_file", "grep", "glob", "write_file", "edit_file", "apply_patch"]
}
~~~

这让 Skill、会话和模型作用域都在设置/资源层可管理，而不是藏在主循环的隐式分支里。
