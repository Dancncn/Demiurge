<div align="center">

<img src="docs/assets/logo.png" width="132" alt="Demiurge" />

# Demiurge · GOAI AgentTeams ECU Test Demo

**面向工程测试的开源 Agent 运行与证据宿主**

`goai` 分支将 Demiurge 的本地工作区、权限确认和 MCP 能力，与官方 AgentTeams 的 Manager / Team / Worker / Human 协同模型结合，构建一条从合成 A2L / MDA 数据到可复核试验报告的演示闭环。

[![License](https://img.shields.io/badge/License-MIT-111827?style=for-the-badge)](LICENSE)
[![AgentTeams](https://img.shields.io/badge/AgentTeams-GOAI%20Track%201-3D8DFF?style=for-the-badge)](competition/agentteams/agentteams-goai.yaml)
[![Python](https://img.shields.io/badge/Python-3.10%2B-3776AB?style=for-the-badge&logo=python&logoColor=white)](competition/agentteams/demo/run.py)

</div>

---

## 场景与问题

在 ECU 环境测试中，工程人员通常需要先确认 A2L 文件是否可用，再从测量工具或回放数据中整理信号，按照试验大纲和流程手册判断测试值是否满足要求，最后制作正式试验报告。文件数量多、重复操作多，异常文件和证据链容易被遗漏。

本分支用一个脱敏的车载冷藏设备控制器场景说明这类能力如何迁移到其他 ECU、台架或日志分析任务。示例中的 A2L、CAN/MDA 回放、ECU、试验文档和测量值均为合成数据。

## 方案概览

```text
A2L / MDA 合成输入
        ↓
数据处理专家 Agent：预筛选、解析、生成 raw-data.xlsx
        ↓
合规流程 Agent：解析试验大纲与流程手册，生成验收模型
        ↓
报告 Agent：按报告规范生成报告、章节校验和证据索引
        ↓
Human Gate：复核拒绝项、高风险操作和签署前置条件
```

AgentTeams 负责多 Agent 协同与治理；Demiurge 负责本地项目工作区、权限确认、MCP 工具宿主和可视化调试。Team Leader 只拆解和编排，不直接生成测量数据、合规模型或正式报告。

## Agent 分工

| Identity | 职责 | 主要产物 |
| --- | --- | --- |
| `goai-manager` | 接收任务、路由团队、汇总结果和待确认事项 | 任务合同、最终摘要 |
| `trial-report-lead` | 拆解任务、安排依赖、协调三个业务 Worker | 子任务与编排状态 |
| `data-processing-expert` | 筛选 A2L、整理 MDA / 回放测量数据、生成 Excel 原始数据 | `a2l-preflight.json`、`raw-data.xlsx` |
| `compliance-process-agent` | 解析试验大纲、流程手册、验收标准和报告章节 | `compliance-model.json` |
| `report-agent` | 组合原始数据、合规模型和报告规范，生成并校验报告 | `trial-report.docx`、`validation-result.json` |
| `human-operator` | 审批高风险桌面操作，复核异常和签署前置条件 | allow / deny / retry / rollback |

## 运行离线 Demo

环境要求：Python 3.10+。在仓库根目录执行：

```powershell
python competition/agentteams/demo/run.py
```

运行不需要 API Key、真实硬件或企业网络。样例会生成：

- 6 个模拟 ECU、72 个合成测量信号；
- 1 个可用 A2L 和 1 个故意损坏的 A2L；
- `raw-data.xlsx`、`compliance-model.json`、`trial-report.docx`；
- `validation-result.json`、`run-evidence.json` 及校验摘要。

结果中两个字段需要分别理解：

| 字段 | 含义 |
| --- | --- |
| `data_verdict = pass` | 原始测量数据按试验大纲的 6 条验收标准全部通过 |
| `verdict = needs_human_review` | 输入文件或签署前置条件仍需人工复核；本样例因为损坏 A2L 保留该状态 |

系统不会因为测量值通过，就静默忽略输入文件质量问题。

## UI 到仓库入口的运行证据

演示中的主运行链路为：

```text
Demiurge UI
  → mcp__goai-ecu-repository__run_goai_demo
  → competition/agentteams/demo/mcp_server.py
  → competition/agentteams/demo/run.py
  → competition/agentteams/demo/outputs/
```

MCP 工具以固定工作目录启动仓库 `run.py`，不把结果写入 C 盘 Demiurge 沙盒，也不接受生产路径或真实车辆数据。调用记录与结果位于：

- [UI 运行截图与说明](competition/agentteams/demo/evidence/README.md)
- [调用记录 `ui-invocation.json`](competition/agentteams/demo/outputs/ui-fridge-ecu-screening/ui-invocation.json)
- [验证结果 `validation-result.json`](competition/agentteams/demo/outputs/ui-fridge-ecu-screening/validation-result.json)
- [运行证据 `run-evidence.json`](competition/agentteams/demo/outputs/ui-fridge-ecu-screening/run-evidence.json)

## AgentTeams 与提交材料

- AgentTeams 资源清单：[agentteams-goai.yaml](competition/agentteams/agentteams-goai.yaml)
- MCP 工具契约：[mcp-contract.yaml](competition/agentteams/mcp-contract.yaml)
- 可复现模拟包：仓库内直接按下方命令运行；ZIP 仅作为本地交付备份，不纳入 Git。
- 操作手册：[OPERATOR-GUIDE.md](competition/agentteams/demo/OPERATOR-GUIDE.md)
- 运行入口与包清单：[package-manifest.json](competition/agentteams/demo/package-manifest.json)
- 仓库提交与评委引导：[competition/submission/README.md](competition/submission/README.md)
- 技术要求和组件边界：[GOAI-ADAPTATION.md](docs/GOAI-ADAPTATION.md)

仓库提交内容包含运行入口、依赖说明、配置、样例输入输出、8 个可复用 Skill、MCP 契约、运行记录和本次 UI 调用证据。方案 PPT、项目一页纸和 ZIP 交付包保留在本地提交材料目录，不混入代码仓库。

## 可选的 LLM 复核模式

离线确定性流程是结果的来源。需要时可使用一次性环境变量启用 DeepSeek 复核：

```powershell
$env:DEEPSEEK_API_KEY = Read-Host "临时输入 DeepSeek API Key"
python competition/agentteams/demo/run.py --llm
Remove-Item Env:DEEPSEEK_API_KEY
```

LLM 只接收合成的中间结果并生成复核摘要；统计值、文件筛选、验收判断和报告校验仍由本地流程完成。不要把 Key 写入仓库、配置文件或 Git 历史。

## 技术边界与公开说明

- 当前交付是可运行的脱敏模拟包，不宣称真实 AgentTeams 集群、企业 MCP、MDA 软件、CAN 卡、TSMaster 或 ECU 台架已经接通。
- 真实业务接入时，A2L、MDF / MF4、试验大纲、流程手册和报告模板必须经过授权、脱敏和环境隔离。
- 示例中的环境测试参数只用于演示数据判定机制，实际判定应以授权试验大纲为准。
- 公开资料索引见 [GOAI-PUBLIC-REFERENCE-AUDIT.md](docs/GOAI-PUBLIC-REFERENCE-AUDIT.md)，开源发布检查见 [OPEN-SOURCE-RELEASE.md](docs/OPEN-SOURCE-RELEASE.md)。

## 验证命令

```powershell
npm test
npm run agentteams:check
python competition/agentteams/demo/test_run.py
```

当前 `goai` 分支验证基线：前端 45 项测试通过、GOAI 演示单元测试 2 项通过、AgentTeams 资源检查通过。

## License

Demiurge is released under the [MIT License](LICENSE). 本仓库中的示例数据、演示文档和证据图仅用于公开模拟与技术说明，不包含真实企业文件或生产测试结果。
