# 运行证据与演示材料

作品演示稿使用可编辑文字和原生表格直接呈现 `raw-data.xlsx`、`validation-result.json` 和报告结论；图片只用于证明 Demiurge UI 确实调用了仓库入口，不使用流程信息图替代实际产物。

## 仓库 run.py 的真实 UI 运行截图（本次交付主证据）

这组截图由 Demiurge UI 调用已连接的本地 MCP 工具完成。工具固定执行仓库中的 `competition/agentteams/demo/run.py`，输出写入仓库 `competition/agentteams/demo/outputs/`，没有把结果写入 C 盘 Demiurge 沙盒，也没有通过对话模型手工编造结果。

执行链：`Demiurge UI → mcp__goai-ecu-repository__run_goai_demo → mcp_server.py → run.py → demo/outputs/`。

| 文件 | 证明内容 |
| --- | --- |
| `runtime-screenshots/10-ui-repository-mcp-connected.jpg` | Demiurge 已连接仓库 MCP，并发现业务工具 |
| `runtime-screenshots/11-ui-repository-prompt-entered.jpg` | UI 输入要求调用仓库 `run.py` 的场景 |
| `runtime-screenshots/12-ui-repository-running.jpg` | UI 中真实显示 `run_goai_demo` 正在执行 |
| `runtime-screenshots/13-ui-repository-final-verdict.jpg` | UI 展示原始数据 6/6 通过，但总状态需要人工复核 |
| `runtime-screenshots/14-ui-repository-final-fields.jpg` | UI 展示 `run.py` 入口、仓库工作目录和仓库输出目录 |

本组截图全部使用合成数据，不含真实 ECU、企业文件或 API Key。对应产物中的 `ui-invocation.json` 记录了调用者、MCP 工具和仓库入口。本目录只保留这 5 张主运行证据，不再混入早期沙盒截图。

本组截图只使用合成输入和脱敏后的中间结果，不包含真实 ECU、企业文件或 API Key；完整模拟链路的实际产物以 `outputs/run-evidence.json` 为准。
