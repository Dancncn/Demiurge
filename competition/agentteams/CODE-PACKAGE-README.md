# Demiurge ECU 试验智链｜AgentTeams 可运行代码包

本包对应 GOAI 赛道一初赛的可选“可执行 AgentTeams 代码包”，包含官方 AgentTeams 声明、8 个 Skill、MCP 契约、离线运行入口、依赖与配置说明、合成样例输入输出和真实 UI 调用证据。

## 最快运行

环境：Python 3.10+。在本包根目录执行：

```powershell
python competition/agentteams/demo/run.py
python competition/agentteams/demo/test_run.py
```

默认模式无第三方 Python 依赖、无需 API Key、无需网络、无需真实 ECU/MDA/CAN 硬件。详细参数见 `competition/agentteams/demo/README.md` 和 `OPERATOR-GUIDE.md`。

## 关键入口

- AgentTeams 声明：`competition/agentteams/agentteams-goai.yaml`
- MCP 契约：`competition/agentteams/mcp-contract.yaml`
- Python Demo：`competition/agentteams/demo/run.py`
- 本地 stdio MCP：`competition/agentteams/demo/mcp_server.py`
- 样例输入：`competition/agentteams/demo/samples/`
- 样例输出与证据：`competition/agentteams/demo/outputs/ui-fridge-ecu-screening/`
- 可复用 Skill：`packs/default/skills/goai-*/SKILL.md`

## 结果解释

- `data_verdict = pass`：合成测量值按虚构试验大纲的 6 条规则全部通过。
- `verdict = needs_human_review`：1 个故意损坏的 A2L 被预筛拒绝，整批材料仍需人工复核。

这两个结论故意分离，防止数据规则通过后掩盖输入文件质量问题。

## 安全与公开边界

本包只含公开资料、虚构文档和合成数据，不含真实企业、车型、ECU、A2L、CAN/MDF、试验阈值、内网地址或密钥。真实 MDA/桌面/台架动作必须由 Human Gate 批准并保留回滚与审计。
