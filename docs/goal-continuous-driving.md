# Goal Continuous Driving

Demiurge 的 Goal 机制参考持续驱动式编码 Agent 的思路：用户设置一个长期目标后，普通回合结束时后端会继续调度 Agent，直到目标完成、暂停、阻塞、超出预算、达到最大回合数或被用户中断。

## Commands

- `/goal <objective>`：设置或替换当前会话目标。目标会持久化到当前 session。
- `/goal <objective> +500k`：设置目标并附带 token budget。支持 `k` / `m` 后缀，也支持 `use 2m tokens` 这类写法。
- `/goal status`：查看目标、状态、已用 / 预算 token、累计活跃时间和续跑回合数（Continuation turns）。
- `/goal pause` / `/goal resume`：暂停或恢复持续驱动。
- `/goal continue`：仅从 `max_turns` 状态恢复（重置续跑计数并继续）。`budget_limited` 只提醒一次，需用户重新 `/goal <objective>` 或 `/goal complete` / `clear`；`usage_limited` 当前没有命令可直接恢复（且该状态为预留态）。
- `/goal complete`：用户手动标记完成。
- `/goal clear`：清除当前目标。

## Model Tool

主 tools schema 中包含 `goal` 工具：

- `{"action":"get"}`：读取当前目标。
- `{"action":"update","status":"complete","reason":"..."}`：模型在完成审计后标记完成。
- `{"action":"update","status":"blocked","reason":"..."}`：模型报告阻塞。相同阻塞原因连续出现 3 次后才会真正进入 `blocked`，避免一次性误判。

模型不能通过工具创建、替换、暂停、恢复或清除目标；这些状态转换保留给用户 slash command。

## Continuation Policy

- 普通用户消息、`/ultracode`、`/compact`、`/dream` 等进入 Agent 回合的请求成功后会触发一次 Goal drive 检查；`/workflows` 本身只列举 run 状态、不进入 Agent 回合，但同样会触发一次 drive 检查。
- Goal drive 会以内部消息 `[Goal continuation #N]` 继续推进，不在前端历史中显示这些内部消息。
- 每个持续回合都会注入当前目标、状态、预算、活跃时间和续跑规则。
- token 用量优先采用 provider 返回的精确 usage（`total_tokens` 或 input+output）；仅当 provider 未返回 usage 时，才用本地启发式估算兜底（覆盖用户输入、助手输出和工具结果）。
- 达到 budget 后会进入 `budget_limited`，只注入一次预算提醒，等待用户重新设置目标。
- 续跑计数 `turns_executed` 达到 `MAX_GOAL_TURNS = 150` 时进入 `max_turns`（含第 150 回合本身）。

## Current Limits

- budget 在 provider 返回 usage 时按精确值计入；仅在缺失 provider usage 时退化为本地估算（精确优先、估算兜底）。
- 替换已有 active goal 时还没有 UI 二次确认。
- 暂停、恢复、预算状态暂未做前端状态栏，只能通过 `/goal status` 查看。
