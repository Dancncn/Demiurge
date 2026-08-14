# GOAI 模拟链路操作手册

本手册给没有参与开发的同伴使用。整个流程只处理本目录中的合成数据，不需要真实 A2L、CAN 日志、车型信息、企业文档、同星硬件或 TSMaster。

## 1. 准备环境

要求：

- Windows PowerShell；
- Python 3.10 或更新版本；
- 一个新的、可随时撤销的 DeepSeek API Key（只在需要 LLM 增强模式时使用）。

从仓库根目录打开 PowerShell：

    cd D:\Project\Project-1\Demiurge

如果使用压缩包，请先解压，并在包含 competition/agentteams/demo 的目录执行后续命令。

## 2. 先运行离线闭环

离线模式不需要 API Key，先执行：

    python competition/agentteams/demo/run.py

预期输出包含：

- selected_a2l: fridge-ecu-good.a2l
- accepted_a2l: 1
- rejected_a2l: 1
- ecu_count: 6
- signal_count: 72
- verdict: needs_human_review

不要把 needs_human_review 当成失败。样例故意放入一个损坏的 A2L，系统应当筛掉它并保留人工复核状态。

## 3. 检查输出

打开 competition/agentteams/demo/outputs：

| 文件 | 检查内容 |
| --- | --- |
| a2l-preflight.json | 是否有 1 个 accepted 和 1 个 rejected |
| raw-data.xlsx | 是否包含 Summary 和 RawData 两个工作表 |
| compliance-model.json | 是否包含温度变化、湿热/结露和 CAN 质量要求 |
| trial-report.docx | 是否能用 Word 打开，标题是否写明模拟演示 |
| validation-result.json | 查看 `acceptance_criteria_checks` 是否逐项为 pass；本样例 `data_verdict` 应为 pass，而总 `verdict` 仍为 needs_human_review，因为故意放入一个损坏 A2L 供人工复核 |
| run-evidence.json | 是否包含 Manager、三个业务 Agent 和 Human 的轨迹 |

## 4. 使用 DeepSeek 增强模式

这一步只把合成的 A2L 预筛选结果、合规模型和校验结果发送给 DeepSeek，模型负责生成复核摘要。统计值、文件筛选和最终校验仍由本地代码完成。

在当前 PowerShell 窗口临时设置 Key：

    $env:DEEPSEEK_API_KEY = Read-Host "临时输入 DeepSeek API Key"
    python competition/agentteams/demo/run.py --llm
    Remove-Item Env:DEEPSEEK_API_KEY

不要把 Key 写入任何仓库文件，不要把 Key 发到聊天中，不要把包含 Key 的终端历史或日志提交到 Git。测试结束后立即在 DeepSeek 平台撤销该 Key。

配置使用：

- Provider：DeepSeek；
- Base URL：https://api.deepseek.com/v1；
- Model：deepseek-v4-flash；
- Endpoint：/chat/completions。

成功后会多出 llm-review.json。只要看到：

    "status": "success"

就说明模型复核环节成功。run-evidence.json 的 llm_mode 应为 true，并包含 llm-review.json 的校验值。

## 5. 失败排查

| 现象 | 处理 |
| --- | --- |
| 未找到 DEEPSEEK_API_KEY | 重新在当前 PowerShell 窗口设置环境变量 |
| HTTP 401 | Key 无效、已撤销或粘贴时多了空格 |
| HTTP 402 | 账户余额或模型权限不足；保留离线运行证据 |
| HTTP 429 | 等待后重试，不要循环刷请求 |
| 网络请求失败 | 检查网络或代理；离线模式仍然可复现 |
| signal_count 不是 72 | 确认执行的是本目录 run.py 和 demo-config.json |
| 文档打不开 | 删除 outputs 后重新运行；不要手工修改二进制文件 |

## 6. MCP 可选验证

如需验证本地业务 MCP：

    python competition/agentteams/demo/mcp_server.py

它是 stdio MCP Server，提供 a2l_preflight、mda_capture_measurements 和 excel_build_raw_data。Demiurge 图形界面中可在 Settings → Tools → MCP 添加：

- Name：goai-ecu-simulator
- Command：python
- Args：competition/agentteams/demo/mcp_server.py
- Transport：stdio
- Enabled：先关闭，保存后 Refresh，再按需开启

MCP Server 只读取合成样例，并限制 XLSX 输出在 demo/outputs 目录。它不连接真实 MDA，也不持有 API Key。

## 7. 交付运行证据

提交或转交时保留：

1. 命令行输出；
2. outputs/run-evidence.json；
3. outputs/a2l-preflight.json；
4. outputs/raw-data.xlsx；
5. outputs/trial-report.docx；
6. 若执行过 LLM 模式，再保留 outputs/llm-review.json；
7. 不要保留 API Key、终端截图中的 Key、真实企业文件或真实硬件日志。
