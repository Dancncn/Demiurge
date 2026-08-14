# GOAI AgentTeams 脱敏模拟运行包

这个目录是赛道 1 初赛可提交的最小可运行示例。它模拟车载冷藏设备 ECU 的温度冲击、高温高湿/结露测试流程，展示：

1. 数据处理专家 Agent：批量预筛选 A2L，保留不可解析文件及原因，解析可用信号目录；
2. MDA 适配层：用合成 CAN/MDA 回放数据替代真实采集硬件；
3. Excel 工具：把统计结果写入可打开的 raw-data.xlsx；
4. 合规流程 Agent：读取虚构的试验大纲和流程手册，生成合规模型，并按大纲中的 acceptance_criteria 对原始 Excel 的每个 ECU/信号统计值逐项判定通过、失败或需要人工复核；
5. 报告 Agent：生成 trial-report.docx、Markdown 报告和校验结果；
6. Manager/Worker 轨迹：生成 run-evidence.json，记录协作、证据和人工复核状态。

所有输入和输出均为合成数据。这个包不连接真实 ECU、TSMaster、同星 CAN 卡、企业网关或企业文件系统，不代表真实环境已经验证。

给测试同伴的逐步操作说明见 OPERATOR-GUIDE.md。

## 运行环境

- Python 3.10+
- 不需要第三方 Python 依赖；requirements.txt 仅用于说明运行时边界。

在仓库根目录运行：

    python competition/agentteams/demo/run.py

也可以指定输出目录：

    python competition/agentteams/demo/run.py --output competition/agentteams/demo/outputs

## 可选 DeepSeek 增强模式

离线模式已经可以完成完整的确定性链路。需要让 DeepSeek 参与“报告复核摘要”时，在当前终端临时配置 Key：

    $env:DEEPSEEK_API_KEY = Read-Host "临时输入 DeepSeek API Key"
    python competition/agentteams/demo/run.py --llm
    Remove-Item Env:DEEPSEEK_API_KEY

配置文件中的模型为 deepseek-v4-flash，接口为 https://api.deepseek.com/v1。增强模式只发送本包生成的合成 A2L 预筛选结果、合规模型和校验结果，不发送真实企业文件；统计值和自动校验仍由本地确定性程序负责，模型不能改变通过/复核结论。成功后新增 llm-review.json。

由于 DeepSeek V4 Flash 默认可能返回思考模式内容，演示请求显式关闭 thinking，仅保存最终复核文本，不把内部推理内容当作报告证据。

不要把 Key 写入 demo-config.json、MCP 配置、PowerShell 脚本、日志或 Git。若同伴使用 Demiurge 图形界面，可在 Settings → Provider → DeepSeek 中配置同一模型，再按本节的临时环境变量方式运行脚本；图形界面的 Key 不会自动暴露给外部 Python 进程。

成功后会生成：

| 文件 | 作用 |
| --- | --- |
| a2l-preflight.json | 每个 A2L 文件的接受/拒绝结果、解析原因和选中的文件 |
| raw-data.xlsx | MDA 回放后按 ECU/信号聚合的最小值、最大值、平均值和样本数 |
| compliance-model.json | 试验大纲、流程步骤和报告要求的结构化结果 |
| trial-report.docx | 可打开的模拟正式报告 |
| trial-report.md | 便于审查的文本版报告 |
| validation-result.json | 试验大纲逐项判定、报告章节、证据链和人工复核状态；`data_verdict=pass` 表示原始数据判定通过，`verdict=needs_human_review` 表示仍有输入文件需要人工确认 |
| run-evidence.json | Agent 协作轨迹、产物校验和运行结论 |
| evidence/runtime-screenshots/*.jpg | Demiurge UI 调用仓库 `run.py` 的实际运行截图 |

样例中故意放入一个结构不完整的 A2L 文件，所以正常结果是 needs_human_review：系统会继续处理可用文件，但不会把被拒绝文件静默吞掉。

## 本地 MCP Server

mcp_server.py 是一个无第三方依赖的本地 stdio MCP 示例，提供：

- `run_goai_demo`：由 Demiurge UI 调用仓库中的 `run.py`，使用包内合成样例并把输出写回仓库 `demo/outputs/`；
- a2l_preflight
- mda_capture_measurements
- excel_build_raw_data

它只允许读取 demo 包内的样例目录，并且只允许向 demo 的 outputs 写入 XLSX。启动：

    python competition/agentteams/demo/mcp_server.py

在 Demiurge 中配置方式：

1. 设置 → Tools/工具 → MCP → Add Server；
2. Name：goai-ecu-simulator；
3. Command：python；
4. Args：competition/agentteams/demo/mcp_server.py；
5. Enabled：先关闭，保存后再点击 Refresh 验证；
6. 也可以参考 mcp-config.example.json。

`run_goai_demo` 是本次 UI 端到端证据的固定入口：它不接收外部路径，不接收生产数据，只能调用本目录的 `run.py` 和样例配置。后续接入真实 MDA 时，只替换 mda_capture_measurements 的实现和数据权限边界，不改变上层 Agent 合同。

## AgentTeams 对照

正式 AgentTeams 资源映射位于上级目录的 agentteams-goai.yaml。本运行包对应：

- Manager：任务分解和结果汇总；
- 数据处理专家 Agent：A2L 预筛选、MDA 回放和原始 Excel；
- 合规流程 Agent：试验大纲和流程手册；
- 报告 Agent：报告生成和校验；
- Human：对不可解析 A2L、规则冲突和最终结论进行审核。

这里的 Python 入口是可复现的本地演示入口，不应描述为已经完成官方 AgentTeams 集群部署。真实部署仍需按 competition/agentteams/README.md 替换 Higress/Matrix/MinIO 等环境占位配置。

## 复用现成 MCP 的判断

Excel 可以考虑复用社区项目，例如 negokaz/excel-mcp-server（MIT，支持 XLSX 读写）或 haris-musa/excel-mcp-server（MIT，偏本地 Excel 文件操作）。接入前需要固定文件范围、禁止路径穿越、限制写入目录并锁定版本。

没有找到可以直接承接本项目 A2L 预筛选、MDA 回放、证据追踪和试验报告合同的成熟通用 MCP。ASAM MDF/MF4 可以用 asammdf 作为后续 Python 数据读取基础，但它是数据处理库，不是本项目所需的完整 MCP Server。因此当前包保留了一个小而明确的本地适配层。

## 安全和开源边界

- 不要放入真实 A2L、MDF/MF4、CAN 日志、企业试验大纲或正式报告模板；
- 不要放入真实车型、ECU 编号、CAN ID、标定值、内网地址或 API Key；
- 温度、湿度和判定值均为演示值，不能当作行业标准；
- 报告中明确写明“模拟演示数据”；
- 第三方 MCP 只作为可选依赖，默认关闭并保留上游许可证说明。
