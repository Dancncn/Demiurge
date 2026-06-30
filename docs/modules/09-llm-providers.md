# LLM Provider 适配层与能力画像

> 存档级技术原理文档。读者：Demiurge 协作开发者。
> 覆盖代码：`src-tauri/src/llm/mod.rs`、`openai.rs`、`anthropic.rs`、`gemini.rs`、`local.rs`、`src-tauri/src/connection_tests.rs`。
> 约定：行号形如 `src-tauri/src/llm/mod.rs:225` 指向撰写本文时的源码状态。

---

## 1. 模块职责与定位

Demiurge 把"对接哪一家大模型"这件事压缩成两层抽象：

1. **能力画像（`ProviderProfile`）** —— 一个纯数据 + 纯函数的结构，回答"这家 provider / 这个模型能做什么、限制是什么、请求字段长什么样"。它是**唯一的能力入口**，由 `ProviderProfile::for_kind` 统一构造（`src-tauri/src/llm/mod.rs:225`）。
2. **适配器（adapter）** —— 三套 HTTP/SSE 方言实现：OpenAI 兼容（`openai.rs`，本地端点 `local.rs` 复用之）、Anthropic（`anthropic.rs`）、Gemini（`gemini.rs`）。每个 adapter 负责构造请求体、解析自家流式协议、把结果归一化为统一的 `AssistantTurn`。

设计动机是：上层（Agent runner、summary、memory、subagent、dream、连接测试）**永远只面对 `Settings + ProviderProfile`，不直接 if-else 判断 provider 字符串**。新增一家 OpenAI 兼容厂商，理论上只需在 `ProviderKind` 加一个枚举值，并在 `for_kind` 的兼容分支里挂上即可，无需改动任何请求构造或流式解析代码。

`stream_completion`（`src-tauri/src/llm/mod.rs:558`）是对外的统一流式入口，根据 `profile.adapter_kind()` 把调用分发到对应 adapter 的 `stream_completion_with_profile`。

```
上层调用方 (runner / summary / memory / subagent / dream)
        │  传入 Settings + Message[] + tools(Value)
        ▼
llm::stream_completion (mod.rs:558)
        │  profile = ProviderProfile::for_kind(cfg.provider)
        │  match profile.adapter_kind()
        ├── OpenAiCompatible & provider==Local ─→ local::stream_completion_with_profile
        ├── OpenAiCompatible                    ─→ openai::stream_completion_with_profile
        ├── Anthropic                           ─→ anthropic::stream_completion_with_profile
        └── Gemini                              ─→ gemini::stream_completion_with_profile
                                                       │
                                                       ▼
                                                 AssistantTurn { content, tool_calls, finish_reason, usage }
```

---

## 2. 关键类型与入口函数

### 2.1 `ProviderProfile`（mod.rs:135-150）

承载所有"能力维度"的不可变结构（`Clone + Copy`），字段含义：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `adapter` | `ProviderAdapterKind` | 决定走哪套 HTTP/SSE 方言 |
| `supports_tools` / `supports_streaming` | `bool` | 是否带 tools、是否流式（当前所有 profile 均为 `true`） |
| `prompt_cache` | `PromptCacheCapability` | prompt cache 能力建模 |
| `thinking` | `ThinkingCapability` | 思考/推理可见性建模 |
| `reasoning_effort` | `ReasoningEffortCapability` | reasoning effort 的传递方式 |
| `parallel_tool_calls` | `ParallelToolCallCapability` | 并行工具调用建模 |
| `requires_api_key` | `bool` | 是否强制要求 API Key |
| `max_input_tokens` / `max_output_tokens` | `Option<u32>` | 模型侧硬上限（用于 clamp） |
| `token_budget_multiplier` | `u32` | 预留字段，当前全部为 `1`，无任何读取点 |
| `tool_schema_dialect` | `ToolSchemaDialect` | 工具 schema 方言 |
| `structured_output` | `StructuredOutputCapability` | 结构化输出能力 |

构造工厂均为 `const fn`，以 `openai_compatible()`（mod.rs:153）为"最小公分母"基底，其余通过结构体更新语法 `..ProviderProfile::openai_compatible()` 派生：

- `openai()`（mod.rs:171）：在兼容基底上叠加 `reasoning_effort = OpenAiChatCompletions`、`parallel_tool_calls = OpenAiCompatibleField`，并把窗口 clamp 到 `max_input_tokens = 272_000` / `max_output_tokens = 128_000`（注释说明对应 GPT-5 系列：总窗 ~400K、输入 ~272K、输出 ~128K）。
- `local_openai_compatible()`（mod.rs:182）：仅把 `requires_api_key` 改为 `false`，其余完全等同兼容基底。
- `anthropic()`（mod.rs:189）：独立 const，`adapter = Anthropic`，开启 `prompt_cache = AnthropicCacheControl`、`thinking = AnthropicThinking`、`reasoning_effort = AnthropicOutputConfig`、`parallel_tool_calls = ProviderManaged`，窗口 200K/64K。
- `gemini()`（mod.rs:207）：`adapter = Gemini`，`thinking = GeminiThinking`、`reasoning_effort = GeminiThinkingBudget`、`parallel_tool_calls = ProviderManaged`，窗口 1_000_000/65_536。

### 2.2 `ProviderProfile::for_kind`（mod.rs:225）—— 单一能力入口

这是全系统唯一从 `ProviderKind` 到能力画像的映射点。值得注意的设计取舍：

- **`ProviderKind::OpenAi`（官方 OpenAI）单独走 `openai()`**，因为它需要 reasoning effort、parallel tool calls 字段和明确的窗口 clamp。
- **十余家 OpenAI 兼容厂商**（DeepSeek、DashScope、OpenRouter、GLM、MiniMax、xAI、Groq、Mistral、Moonshot、Perplexity、Doubao、Hunyuan、StepFun、Custom、OpenAiCompatible）**共用 `openai_compatible()`**，即默认不发 OpenAI 专属字段、不 clamp 窗口（`max_input_tokens = None`，尊重用户手填值）。
- `Local` 走 `local_openai_compatible()`（免 Key）。
- `Anthropic` / `Gemini` 各自走专属 const。

这种"官方与兼容分流"的关键后果体现在 token 预算上（见 §3.4）：官方 OpenAI 声明了输入硬上限 `272_000`、输出硬上限 `128_000`（mod.rs:176-177），因此用户 250K 的输入设置不会被裁剪（`min(250_000, 272_000) = 250_000`，保持 250K），只有超过 272K 时才会被 clamp 到 272K；而 OpenAI 兼容厂商（`max_input_tokens = None`）则完全不 clamp、原样保留用户设置。兼容侧的"原样保留"由测试 `openai_compatible_profile_keeps_provider_defined_limits`（mod.rs:714）固化。

### 2.3 `AssistantTurn` 与 `Usage`（mod.rs:14-61）

`AssistantTurn` 是所有 adapter 的统一返回值：

```rust
pub struct AssistantTurn {
    pub content: String,                                 // 可见正文
    pub tool_calls: Vec<crate::agent::conversation::ToolCall>,
    pub finish_reason: String,                           // 归一化后：stop|tool_calls|length|content_filter|interrupted|...
    pub usage: Option<Usage>,                            // 归一化 token 用量
}
```

`Usage`（mod.rs:15-20）只保留 `input_tokens / output_tokens / total_tokens` 三个 `Option<usize>`，屏蔽各家字段名差异。

---

## 3. 核心数据流与算法

### 3.1 流式解析的统一骨架（SSE 行缓冲状态机）

三个 adapter 的 `stream_completion_with_profile` 共享同一套**字节流 → 按 `\n` 切行 → 取 `data:` 前缀 → 喂解析器**的骨架（`openai.rs:43-73`、`anthropic.rs:69-98`、`gemini.rs:67-93`）：

```
loop over bytes_stream():
    if cancel.load(Relaxed):           # 用户中断
        state.finish = "interrupted"; break
    buf.extend(chunk)
    while buf 含有 '\n':
        line = buf.drain(..=pos)       # 取出一整行（含换行）
        line = line.trim()
        data = line.strip_prefix("data:")?  # 非 data 行跳过
        if data.is_empty(): continue
        parse_*_stream_data(data, &mut state, &mut on_delta)
```

差异点（终止条件）：

- **OpenAI**：遇到 `data: [DONE]` 时 `break 'outer`（openai.rs:66）。
- **Anthropic**：解析到 `message_stop` 事件后置 `state.message_stopped = true`，外层 `break 'outer`（anthropic.rs:92、anthropic.rs:306）。
- **Gemini**：无显式终止哨兵，靠 `streamGenerateContent?alt=sse` 流自然结束（gemini.rs:71 的 `while let Some`）。

`on_delta` 回调在每段可见正文增量上触发，用于把 token 实时推送到前端（cancel 检查在每个 chunk 边界做，保证中断的最长延迟是一个网络块）。

### 3.2 中断（cancel）语义

`cancel: &AtomicBool` 一旦置位，循环在下一个 chunk 边界把 `state.finish` 改写成 `"interrupted"` 并跳出。随后 `normalize_finish_reason` 会把它原样保留为 `"interrupted"`（见 §3.6 各 adapter 分支均显式列了 `"interrupted" => "interrupted"`）。注意：**已经累积的 `content` 和 `tool_calls` 仍会随 `AssistantTurn` 返回**，中断不丢弃已收到的部分输出。

### 3.3 `merge_usage` —— 跨事件累加用量（mod.rs:33-52）

流式协议中 usage 往往**分多次到达**（Anthropic 在 `message_start` 给 input、在 `message_delta` 给最终 output；OpenAI/Gemini 通常在末尾一次给齐）。`merge_usage` 提供"后到字段优先、缺失字段回退"的合并：

```rust
pub(crate) fn merge_usage(slot: &mut Option<Usage>, next: Usage) {
    *slot = Some(slot.map(|current| current.merge(next)).unwrap_or(next));
}
```

`Usage::merge`（mod.rs:33-47）逐字段 `next.or(self)`，且 `total_tokens` 在两侧都缺失时尝试用 `input + output` 重算（`saturating_add` 防溢出）。`total_or_sum`（mod.rs:23-31）则是读取侧的兜底：优先用 `total_tokens`，否则按可用的 input/output 求和。

### 3.4 token 预算的 clamp 算法（effective_*）

预算入口是 `effective_token_budget`（mod.rs:399），返回 `ProviderTokenBudget { max_input_tokens, reserved_output_tokens }`，分别由两个 helper 计算：

```rust
// mod.rs:385
pub fn effective_max_input_tokens(self, settings) -> usize {
    self.max_input_tokens
        .map(|limit| settings.max_input_tokens.min(limit as usize))  // 有硬上限则取 min
        .unwrap_or(settings.max_input_tokens)                        // 无上限则原样
        .max(1)                                                      // 至少 1
}
// effective_reserved_output_tokens（mod.rs:392）同理，对 max_output_tokens clamp
```

要点：

- **clamp 只在 profile 声明了 `max_*_tokens` 时生效**。OpenAI 兼容厂商（`None`）不 clamp，把窗口管理权交还用户与设置（兼容厂商的真实窗口千差万别，硬编码反而会误伤）。
- `.max(1)` 保证下游不会拿到 0 预算。

对官方 OpenAI 而言，输入硬上限为 `272_000`、输出硬上限为 `128_000`（mod.rs:176-177）：用户设置 250K 输入 / 32K 输出时，两侧都低于硬顶，因此都原样保留（输入 250K、输出 32K），均不会被裁剪。

`effective_token_budget` 的真正消费者是 `agent/budget.rs` 的 `history_budget_for_profile`（`src-tauri/src/agent/budget.rs:139-172`）：它在 profile 给出的 `max_input_tokens` / `reserved_output_tokens` 基础上，再扣除 system prompt、tools schema、保底 `MIN_HISTORY_BUDGET_TOKENS`，算出"历史消息能占多少 token"，用于历史裁剪与滚动摘要。**历史上 budget 曾作为硬约束的状态已不存在**——当前 profile 侧只做软性 clamp（取 min + 保底），真正决定截断的是 budget 模块，而非在 adapter 请求体里写死。adapter 侧唯一写入请求的限额是 `effective_reserved_output_tokens(cfg)`，作为各家的 `max_tokens` / `max_completion_tokens` / `maxOutputTokens`。

> 注：`effective_max_output_tokens(requested)`（mod.rs:406）和字段 `token_budget_multiplier` 目前**仅有 `#[allow(dead_code)]` 定义和单元测试，无生产调用点**，属于预留扩展位。

### 3.5 reasoning effort 的 profile-gated 路径

这是本模块设计最精细的一条数据流，分三道闸门：

```
settings.reasoning_effort (Auto/Low/Medium/High/Xhigh/Max)
        │
        ▼ 闸门1：profile.supports_reasoning_effort()  —— provider 是否支持（mod.rs:281）
        │         兼容厂商 reasoning_effort=Unsupported，直接 None
        ▼ 闸门2：supports_reasoning_effort_for_model(model)（mod.rs:288）
        │         按 capability 分支做模型名匹配；env 可全局强开
        ▼ 闸门3：effective_reasoning_effort(settings)（mod.rs:310）
        │         env 覆盖 > settings；若结果 is_auto() 则 None（不发任何字段）
        ▼ 各 provider 专属映射函数 → 请求体字段
```

**闸门2 的模型名匹配规则**（mod.rs:288-308）：

- OpenAI（`openai_model_supports_reasoning_effort`，mod.rs:468）：模型名（去掉 `openai/` 前缀）以 `o1`/`o3`/`o4`/`gpt-5` 开头，或包含 `codex`。
- Anthropic（`anthropic_model_supports_reasoning_effort`，mod.rs:501）：包含 `opus-4-7`/`opus-4.7`/`opus-4-6`/`opus-4.6`/`sonnet-4-6`/`sonnet-4.6`/`deepseek-v4-pro`。
- Gemini（`gemini_model_supports_thinking_budget`，mod.rs:511）：包含 `gemini-2.5`/`gemini-3`/`thinking`。

**环境变量旁路**（中立记录）：`env_always_enable_effort`（mod.rs:446）读取 `DEMIURGE_ALWAYS_ENABLE_EFFORT`，命中真值时跳过模型名匹配直接放行（用于尚未进入匹配清单的新模型）。`env_reasoning_effort_override`（mod.rs:432）读取 `DEMIURGE_EFFORT_LEVEL` 覆盖 `settings.reasoning_effort`。两个函数在 `#[cfg(test)]` 下都硬返回 `None`/`false`，保证测试不受宿主环境污染。

**各 provider 的字段映射**（同一逻辑等级，落到不同请求字段）：

| Provider | 函数 | 请求字段 | 等级映射特点 |
| --- | --- | --- | --- |
| OpenAI | `openai_chat_reasoning_effort`（mod.rs:321） | `reasoning_effort: "low"/"medium"/"high"/"xhigh"` | `Xhigh`/`Max` 合并：仅当模型支持时发 `"xhigh"`，否则降级 `"high"` |
| Anthropic | `anthropic_output_config_effort`（mod.rs:343） | `output_config.effort: "low".."max"` | 直通五档，`Max` 保留为 `"max"` |
| Gemini | `gemini_thinking_budget_tokens`（mod.rs:360） | `thinkingConfig.thinkingBudget`（整数 token） | 把等级翻译成思考预算 token 数 |

**OpenAI 的 xhigh 细则**（`openai_model_supports_xhigh_reasoning_effort`，mod.rs:477）：

- `gpt-5-pro` 不支持 xhigh（显式排除）。
- 含 `codex` 的模型支持。
- `gpt-5.x` 系列：当次版本号 `>= 2` 才支持（`openai_gpt5_minor_version` 解析 `gpt-5.` 后的数字，mod.rs:492）。

因此 `gpt-5.2`/`gpt-5.1-codex-max` 支持 xhigh，而 `gpt-5.1`/`gpt-5-pro` 不支持（测试 `openai_xhigh_support_respects_model_exceptions`，mod.rs:797 固化）。

**Gemini thinking budget 的预算反算**（mod.rs:360-383），是这里唯一带二次 clamp 的分支：

```
max_output = effective_reserved_output_tokens(settings)
if max_output <= 2_048: return None                        # 输出窗太小，不开思考
response_reserve = 1_024
max_budget = max_output - response_reserve                 # 给回答留 1024
desired = match effort { Low:1024, Medium:4096, High:8192, Xhigh:16384, Max:32768 }
return desired.min(max_budget).max(1_024)                  # 夹在 [1024, max_budget]
```

即"想要的思考预算"会被实际输出窗口压缩，且永远给最终回答预留 1024 token。测试 `gemini_body_includes_thinking_budget_for_effort`（gemini.rs:351，12000 输出窗 + High → 8192）验证了这条路径。

### 3.6 `normalize_finish_reason` —— 终止原因方言归一化（mod.rs:515）

把各家原始 finish/stop reason 折叠到统一词表 `stop | length | tool_calls | content_filter | interrupted`，优先级最高的是"有工具调用就一定是 `tool_calls`"：

```rust
if has_tool_calls { return "tool_calls"; }   // 覆盖一切原始 reason
```

随后按 adapter 分支翻译（空串一律 `"stop"`，未识别值降为小写原样透传）：

| Adapter | 原始值 | 归一化 |
| --- | --- | --- |
| OpenAI | `stop` / `length` / `tool_calls`\|`function_call` / `content_filter` / `interrupted` | 同名（function_call→tool_calls） |
| Anthropic | `end_turn`\|`stop_sequence` → `stop`；`max_tokens` → `length`；`tool_use` → `tool_calls` | |
| Gemini | `STOP`/`stop` → `stop`；`MAX_TOKENS` → `length`；`SAFETY`/`BLOCKLIST`/`PROHIBITED_CONTENT`/`SPII` → `content_filter` | |

测试 `finish_reason_normalization_matches_adapter_dialects`（mod.rs:820）含一个关键用例：Gemini `STOP` + `has_tool_calls=true` 仍返回 `tool_calls`，证明 `has_tool_calls` 短路优先。

---

## 4. 各 adapter 的请求体差异

三家在消息结构、工具表达、推理字段上分歧很大，统一由各自的 `build_*_body` 处理。下表对照核心差异：

| 维度 | OpenAI 兼容 | Anthropic | Gemini |
| --- | --- | --- | --- |
| 端点 | `{base}/chat/completions` | `{base}/messages` | `{base}/models/{model}:streamGenerateContent?alt=sse&key=` |
| 鉴权 | `Authorization: Bearer`（Local 可省） | `x-api-key` + `anthropic-version: 2023-06-01` | URL query `key=` + （连接测试用 header `x-goog-api-key`） |
| system | 普通 `role:system` 消息 | 抽出所有 system 段，`"\n\n".join` 进顶层 `system` 字段 | 抽出进 `systemInstruction.parts` |
| 消息容器 | `messages`（原样透传 `Message`） | `messages`，user/assistant 转成 content block 数组 | `contents`，role 改名（assistant→`model`，tool→`function`） |
| 工具调用回填 | 依赖 `Message` 自带结构 | `tool_use` block（input 由 arguments 字符串 parse 成对象，失败回退 `{}`） | `functionCall { name, args }` |
| 工具结果 | `role:tool` 原样 | `role:user` + `tool_result` block | `role:function` + `functionResponse`（content 尝试 parse 成对象，否则包成 `{content: ...}`） |
| 输出上限字段 | `max_completion_tokens`（仅 OpenAiChatCompletions profile）否则 `max_tokens` | `max_tokens` | `generationConfig.maxOutputTokens` |
| 并行工具 | `parallel_tool_calls: true`（仅 `OpenAiCompatibleField` profile，即官方 OpenAI） | provider 托管，无字段 | provider 托管，无字段 |
| reasoning 字段 | `reasoning_effort` 字符串 | `output_config.effort` + header `anthropic-beta: effort-2025-11-24` | `generationConfig.thinkingConfig.{includeThoughts, thinkingBudget}` |

**OpenAI 输出上限字段的二选一逻辑**（openai.rs:98-105）：只有 `reasoning_effort = OpenAiChatCompletions` 的 profile（官方 OpenAI）发 `max_completion_tokens`，其余兼容厂商发传统 `max_tokens`——因为 reasoning 系模型 API 要求新字段，而老兼容端点只认旧字段（对照测试 `official_openai_body_uses_profile_specific_fields` 与 `openai_compatible_body_omits_openai_only_fields`，openai.rs:263/280）。注意官方 OpenAI 的输出硬上限为 `128_000`，因此 32K 的 `reserved_output_tokens` 设置会原样写入 `max_completion_tokens`（`min(32_000, 128_000) = 32_000`，不会被压到更小的值）。

**Anthropic 的 effort header**（anthropic.rs:54-56）：仅当 `anthropic_output_config_effort` 返回 `Some` 时才追加 beta header `effort-2025-11-24`，避免无谓地触发实验特性。

**Anthropic / Gemini 强制要求 Key**：两者的 `stream_completion_with_profile` 在 `require_api_key` 之后再 `.ok_or_else(...)`（anthropic.rs:45、gemini.rs:44），即便 profile 理论上放行也会因没 Key 报错——因为这两家不存在免 Key 的本地形态。

### 4.1 工具调用累积的解析差异

- **OpenAI**（openai.rs:199-215）：tool_calls 以 `index` 分片增量到达，用 `BTreeMap<u64, (id, name, arguments)>` 累积，name/arguments 用 `push_str` 拼接。最终 id 缺失时合成 `call_{idx}_{name}`，arguments 空时填 `"{}"`。
- **Anthropic**（anthropic.rs:263-297）：`content_block_start` 建立 tool_use 块（id/name + 可能的初始 input 对象），`input_json_delta` 把 `partial_json` 增量拼到该块的 input。
- **Gemini**（gemini.rs:249-264）：`functionCall` 是完整对象（非增量），直接 push 进 `Vec<ToolCall>`，id 合成 `call_{idx}_{name}`，args 序列化为字符串。Gemini 还会**跳过 `part.thought == true` 的思考片段**（gemini.rs:240），不让思考内容污染可见 `content`（测试 `gemini_stream_omits_thought_parts_from_visible_text`，gemini.rs:390）。

### 4.2 usage 解析的字段差异

- OpenAI（openai.rs:223）：`prompt_tokens` / `completion_tokens` / `total_tokens`。
- Anthropic（anthropic.rs:311）：input 侧把 `input_tokens` + `cache_read_input_tokens` + `cache_creation_input_tokens` **三者相加**（prompt cache 的命中/写入也计入输入），output 用 `output_tokens`，total 自行求和。
- Gemini（gemini.rs:272）：`promptTokenCount` / `candidatesTokenCount` / `totalTokenCount`。

三者都用 `.filter(|u| u.total_or_sum().is_some())` 丢弃全空的 usage 对象。

### 4.3 空工具 schema 的方言占位（mod.rs:420）

当 `profile.supports_tools == false` 时，runner 用 `profile.empty_tool_schema()` 给请求体放一个合法空壳：Gemini 需要 `[{"function_declarations": []}]`，OpenAI/Anthropic 用空数组 `[]`。这保证下游 `build_*_body` 的 `supports_non_empty_tools` 判定一致（空数组 → 不写 tools 字段）。

---

## 5. 连接测试如何复用同一 profile/adapter 路由

`connection_tests.rs` 的核心设计是**与真实推理路径共用能力画像与 adapter 分类**，从而保证"测试通过 ≈ 实际能跑"。

`ProviderTestRequest::from_settings`（connection_tests.rs:180）：

```rust
let profile = ProviderProfile::for_kind(settings.provider);       // 同一能力入口
let api_key = llm::require_api_key(&settings, profile)?...;        // 复用同一 Key 校验
let base_url = normalize_base_url(&settings.base_url)?;            // 校验 http/https + 去尾斜杠
let kind = ProviderTestKind::from_adapter(profile.adapter_kind()); // 复用 adapter 分类
```

`ProviderTestKind`（connection_tests.rs:21）是 `ProviderAdapterKind` 的一对一镜像，`from_adapter`（connection_tests.rs:29）做无损映射。因此**连接测试的路由完全由同一份 `for_kind` 决定**——DeepSeek 走 OpenAI 兼容、Anthropic 走 messages、Gemini 走 generateContent，与流式路径分流逻辑一致（测试 `provider_request_trims_and_routes_openai_compatible` / `provider_request_routes_anthropic_and_gemini`，connection_tests.rs:572/599）。

差异点在于**测试发的是"最小请求"而非流式**：

- OpenAI（connection_tests.rs:215）：`stream:false`、`max_tokens:1`、一条 `"ping"`。
- Anthropic（connection_tests.rs:224）：`max_tokens:1`、content block 形式的 `"ping"`，强制要求 Key。
- Gemini（connection_tests.rs:235）：`maxOutputTokens:1`，打到非流式的 `:generateContent`，鉴权改用 header `x-goog-api-key`（与流式的 URL query `key=` 不同）。

`require_api_key` 的复用还保证了**免 Key 语义一致**：`Local` 在连接测试里也允许空 Key（测试 `provider_request_allows_local_without_key`，connection_tests.rs:585），而远程 provider 缺 Key 直接报错。

### 5.1 Web Search 连接测试（独立子系统）

`test_web_search`（connection_tests.rs:125）与 LLM provider 测试并列但**不复用 `ProviderProfile`**，它有自己的 `WebSearchAdapter` 枚举（Auto/Bing/DuckDuckGo/Tavily/Brave/Exa）：

- **Auto**：先试 Bing，失败回退 DuckDuckGo（connection_tests.rs:133-148）。
- **公共引擎**（Bing/DuckDuckGo）：无需 Key。
- **带 Key 引擎**（Tavily/Brave/Exa）：Key 来源是"设置字段 → 环境变量"二级回退（`setting_or_env`，connection_tests.rs:492），`validate_keys`（connection_tests.rs:300）在发请求前就拦截缺 Key 情况。
- 端点可由环境变量覆盖（`TAVILY_SEARCH_URL` / `EXA_MCP_URL` 等），否则用内置默认值（connection_tests.rs:291-294）。

这部分与 LLM adapter 层解耦，仅共用 `Settings` 与 reqwest client。

---

## 6. 安全与权限相关点

1. **API Key 不落盘**：`Settings.api_key` 注释明确"只保留在内存和前端表单，落盘时清空"（store/mod.rs:222），实际密钥由 `credentials` 模块写系统凭据管理器；`redacted_settings`（store/mod.rs:427）在序列化前清空各类 key。
2. **错误体截断**：连接测试把上游错误体用 `cap_chars` 截到 600 字符（`CONNECTION_TEST_MAX_ERROR_CHARS`，connection_tests.rs:10、542），避免把超长上游响应原样回传/记录。
3. **Base URL 协议校验**：`normalize_base_url`（connection_tests.rs:470）强制 `http`/`https`，拒绝 `ftp://` 等（测试 connection_tests.rs:635）。
4. **Gemini Key 出现在 URL**：流式路径把 `key` 拼进 URL query（gemini.rs:46-51），这是 Gemini API 形态决定的；连接测试则改用 header，二者都不会把 Key 写入持久化设置。
5. **超时**：连接测试统一 20 秒超时（`CONNECTION_TEST_TIMEOUT_SECS`，connection_tests.rs:9）；流式推理路径不设固定超时，靠 `cancel` 中断。

---

## 7. 已知限制与扩展点

| 项 | 当前状态 |
| --- | --- |
| **结构化输出（structured output）** | **已建模但未接通**。`StructuredOutputCapability` / `StructuredOutputRequest` 及三个 `build_*_body_with_structured_output` 函数均带 `#[allow(dead_code)]`，全仓库**无任何生产调用点**——流式入口 `build_*_body` 一律以 `None` 调用它们。能力画像已正确声明各家方言（OpenAI `response_format.json_schema`、Anthropic 转 forced tool、Gemini `responseSchema`），属随时可启用的预留实现。 |
| **prompt cache** | profile 已建模（`PromptCacheCapability`），且 Anthropic usage 解析已把 cache token 计入 input（anthropic.rs:316-318）；但**请求体侧未注入任何 `cache_control` 标记**，即尚未主动"写缓存"，仅被动统计命中。`supports_prompt_cache` 带 `#[allow(dead_code)]`。 |
| **thinking 可见性** | `ThinkingCapability` 已建模，Gemini 实际消费（思考预算 + 过滤 thought 片段）；Anthropic thinking 仅经由 `output_config.effort` 间接体现，未单独发 `thinking` 块。`supports_thinking` 带 `#[allow(dead_code)]`。 |
| **`token_budget_multiplier`** | 全部 profile 恒为 `1`，**无任何读取点**，纯预留字段。 |
| **`effective_max_output_tokens(requested)`** | 仅测试调用，无生产调用点（mod.rs:406）。 |
| **`supports_streaming`** | 所有 profile 恒 `true`；`build_*_body` 仍按字段读取，为将来非流式探测留口。 |
| **OpenAI 兼容厂商无窗口 clamp** | `max_input_tokens = None` 是有意为之（兼容厂商窗口差异巨大），代价是窗口正确性依赖用户/设置而非代码兜底。 |
| **新模型识别滞后** | reasoning effort 模型匹配是硬编码字符串前缀/包含（mod.rs:468-513），新模型上线需改码或用 `*_ALWAYS_ENABLE_EFFORT` 环境变量临时强开。 |

**扩展新 provider 的标准路径**：① 在 `ProviderKind` 增枚举值（store/mod.rs:122）；② 若是 OpenAI 兼容厂商，仅在 `for_kind` 兼容分支挂上，并在 `provider_label`（connection_tests.rs:508）补显示名；③ 若是全新方言，新增 `ProviderAdapterKind`/`ToolSchemaDialect` 变体、对应 `build_*_body` 与流式解析、`normalize_finish_reason` 分支，并在 `stream_completion` 的 match 加路由。能力画像作为单一入口，使②类扩展几乎零成本。
