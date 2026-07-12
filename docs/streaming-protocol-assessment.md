# OpenAI / Anthropic 流式输出协议评估

评估日期：2026-07-12

> 文档状态：已按 `src-tauri/src/llm/sse.rs`、两类适配器与 `src/components/MarkdownRenderer.tsx` 的当前实现复核。相关项目级发现见 [代码审查报告](./CODE-REVIEW-2026-07-12.md)。

## 结论

Demiurge 的前端渲染不直接依赖 OpenAI 或 Anthropic 的原始事件格式。两个供应商适配器先把正文与思考增量归一成 `StreamDelta::Content` / `StreamDelta::Reasoning`，会话引擎再统一发出 `assistant-delta` / `assistant-reasoning` 事件。前端只消费这套统一事件，因此在适配器正确解析的前提下，切换供应商不会改变 Markdown、代码块、公式、Mermaid、自动滚动或文字动画的渲染路径。

本次审计发现，真正可能造成“不同供应商看起来渲染不一致”的风险位于前端之前：SSE 网络分片、流尾事件、流内错误、工具参数增量与停止原因的解析。相关风险已在公共 SSE 解码层和两个供应商适配器中加固。

结论的适用边界是“当前 Chat Completions / Messages 适配器输出的统一增量”。它不代表所有兼容网关都严格遵循同一事件字段，也不覆盖尚未接入的其他响应接口。前端不感知供应商协议，但后端适配器仍必须为每一种新方言增加明确映射与测试。

## 数据流

```text
OpenAI / Anthropic HTTP SSE
        ↓ 供应商事件解析
StreamDelta::Content | StreamDelta::Reasoning
        ↓ 会话引擎统一事件
assistant-delta | assistant-reasoning | assistant-done | assistant-error
        ↓ requestAnimationFrame 合并增量
React 消息状态
        ↓
统一 Markdown 渲染 + 流式文字淡入
```

## 协议差异及归一化

| 语义 | OpenAI Chat Completions | Anthropic Messages | 归一化结果 | 对前端的影响 |
| --- | --- | --- | --- | --- |
| 正文增量 | `choices[0].delta.content` | `content_block_delta` + `text_delta` | `StreamDelta::Content` | 无差异，进入同一个正文缓冲 |
| 思考增量 | `delta.reasoning_content`（推理型兼容端点） | `thinking_delta.thinking` | `StreamDelta::Reasoning` | 无差异，进入独立思考折叠区 |
| 工具调用 | `delta.tool_calls[index]`，名称和 JSON 参数均可分片 | `tool_use` 块 + `input_json_delta` | 完整 `ToolCall` | 不混入正文；工具开始后显示统一活动卡片 |
| 正常结束 | `[DONE]` / `finish_reason` | `message_stop` / `stop_reason` | `stop`、`tool_calls`、`length` 等统一原因 | `assistant-done` 行为一致 |
| 用量 | `prompt_tokens` / `completion_tokens` | `input_tokens` / `output_tokens`，含缓存字段 | `Usage` | 只影响预算统计，不影响文字渲染 |
| 错误 | JSON `error` 或命名 `event: error` | `type: error` 或命名 `event: error` | `Err` → `assistant-error` | 统一显示错误卡片，不再出现“空白成功回复” |

## 已修复的风险

### 1. SSE 分片不能等同于事件边界

`reqwest::bytes_stream()` 返回的是传输层任意字节块，可能在 UTF-8 字符、行尾或 JSON 中间断开。公共 `SseDecoder` 现在按字节缓存并支持：

- 任意 chunk 边界；
- `LF`、`CRLF` 和 `CR` 行尾；
- SSE 注释/心跳行；
- 多个 `data:` 行按规范用换行拼接；
- 同一网络块内的多个事件；
- 最后一个事件没有空行或换行就结束的情况；
- OpenAI 的 `[DONE]` 与 Anthropic 的 `message_stop`。

这消除了“某供应商偶尔少最后几个字”“中文字符被截断”“代理合并 SSE 行后无法解析”等网络层差异。

### 2. 流内错误以前可能被当成空响应

HTTP 连接成功并不代表流一定成功。供应商可能在 SSE 中发送限流、过载或上游断连错误。本次改动会识别 OpenAI/Anthropic 的内嵌错误对象和命名错误事件，并将其送入统一错误路径。

### 3. 工具调用参数必须按协议累积

OpenAI 以 `tool_calls[index]` 为键累积调用 id、函数名与参数；Anthropic 以内容块 `index` 累积 `tool_use` 与 `partial_json`。两个适配器都只在增量完成后构造工具调用，工具 JSON 不会泄漏到可见正文或 Markdown 渲染器。

### 4. 前端高频增量的性能与动画

前端按动画帧合并正文/思考增量，避免每个 token 都触发完整 Markdown 重解析。流式 Markdown 对新挂载的词片段执行轻量淡入，既有文本使用稳定 key，不会每帧重播；系统启用 `prefers-reduced-motion` 时动画自动关闭。未闭合代码围栏会被临时闭合，避免代码块在流式过程中反复变形。

## 剩余边界与建议

1. 当前 OpenAI 官方适配走 `/chat/completions`，不直接支持 Responses API 的 `response.output_text.delta` 事件。如果未来切到 `/responses`，应新增独立适配器并继续输出现有 `StreamDelta`，不能让前端直接消费 Responses 事件。
2. 部分“OpenAI-compatible”服务会使用非标准思考字段（例如 `reasoning` 或嵌套的 reasoning details）。当前标准正文不受影响；若需要显示这些服务的思考过程，应按供应商 profile 显式补映射，避免把未知字段猜成正文。
3. Anthropic 的 `signature_delta` 与 redacted thinking 元数据不应渲染给用户；当前实现会忽略它们，这是有意的行为。
4. 解析器现在会把格式损坏的 JSON 事件作为错误返回。相比静默丢弃，这更容易诊断代理/网关不兼容，但也意味着非标准端点必须输出有效 SSE JSON。
5. 单元测试覆盖协议与分片逻辑；仍建议在发布前分别使用一个真实 OpenAI 模型与一个真实 Anthropic 模型做网络契约测试，覆盖正文、思考、工具调用、取消和限流五种场景。
6. 第三种供应商适配器仍使用独立的按行解析器，尚未获得公共解码器的无换行流尾、多行 data 与显式流内错误保障；这不会改变前端 Markdown 组件，但可能形成空回复、尾段丢失或错误被吞。
7. 主时间线已迁移到带 `turn.session_id` 的统一事件信封，确认与 Goal 事件也携带 session id；剩余问题是 `assistant-done` 只在累计正文为空时回填完整文本，仍应把 done 文本作为 canonical value。
8. 流式 Mermaid 围栏会随增量反复启动解析/渲染；历史消息也可能因不稳定回调重渲染。协议适配正确不代表长会话渲染性能已经闭环。

## 验证矩阵

| 场景 | OpenAI | Anthropic | 自动化覆盖 |
| --- | --- | --- | --- |
| 正文增量 | 是 | 是 | parser tests |
| 思考与正文分离 | 是 | 是 | provider tests |
| 工具名/参数分片 | 是 | 是 | provider tests |
| 使用量合并 | 是 | 是 | provider tests |
| 字节级分片、CRLF、多行 data | 是 | 是 | SSE + provider tests |
| 流尾无换行 | `[DONE]` | `message_stop` | provider tests |
| 内嵌/命名错误事件 | 是 | 是 | provider tests |
| 前端生产构建 | 供应商无关 | 供应商无关 | `npm run build` |
| 漏掉中间 delta 后由 done 自愈 | 供应商无关 | 供应商无关 | **缺测试；当前未保证** |
| 跨会话/turn 事件隔离 | 供应商无关 | 供应商无关 | **统一信封已定义，主时间线未迁移** |
| 流式 Mermaid 只在完成后渲染 | 供应商无关 | 供应商无关 | **缺测试；当前会重复渲染** |

2026-07-12 本地验证：前端生产构建通过，Rust 255 项测试通过，其中包含公共 SSE 解码、字节级分片、CR/LF、多行 data、无换行流尾、流内错误、工具参数累积、usage 与思考/正文分离，以及后续安全边界专项回归。未执行真实外部端点请求，因此限流、代理缓冲、连接中断与供应商线上变更仍属于发布前网络契约测试范围。
