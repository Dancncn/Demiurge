# GOAI AgentTeams 部署包

本目录是 `goai` 分支的官方 AgentTeams 适配入口。

AgentTeams 官方仓库：<https://github.com/agentscope-ai/AgentTeams>

本目录同时包含初赛可运行的脱敏模拟包和后续部署入口。模拟包不代表真实环境已经部署完成，不包含真实 MCP Server、API Key、Matrix 密码、企业内网地址或 ECU/MDA 实测结果；这些内容在后续阶段按实际环境补齐。

## 初赛可运行模拟包

demo/ 是一个 Python 3.10+、无第三方依赖的本地运行包，使用合成 A2L、CAN/MDA 回放和试验文档，输出原始 Excel、正式报告、验证结果和运行证据：

    python competition/agentteams/demo/run.py

它故意包含一个不可解析的 A2L 文件，正常结论为 needs_human_review，用于展示“智能筛选 + 不吞掉异常 + 人工复核”的闭环。详细说明、依赖、样例和 MCP 配置见 demo/README.md。

如需让 DeepSeek 复核合成中间结果，先在当前终端临时设置 DEEPSEEK_API_KEY，再运行 python competition/agentteams/demo/run.py --llm；Key 不写入仓库，详细操作见 demo/README.md。

作品简介、项目一页纸、Agent Identity 和 Skill 清单位于外层初赛提交包；公开资料边界见 ../../docs/GOAI-PUBLIC-REFERENCE-AUDIT.md。

## 前置条件

- AgentTeams `v1.2.2` 或更新的兼容版本；
- Docker Desktop（本地嵌入式部署）或 Kubernetes + Helm（企业部署）；
- 已配置可用的 LLM Provider；
- 已通过 Higress 暴露 ECU 工具 MCP Server；
- MCP Gateway URL、Worker 模型和 Human Matrix 域名已经由部署方确认。

## 后续部署（复赛 / 实测）

按照 AgentTeams 官方安装器完成初始化，然后将 `agentteams-goai.yaml` 中的占位 MCP 地址替换为真实 Higress Gateway 地址。

```powershell
# 官方安装完成后，在 AgentTeams CLI 可用的环境中执行
agt apply -f competition/agentteams/agentteams-goai.yaml
```

资源文件已经按照官方依赖顺序排列：Manager → Workers → Team → Human。其中 4 个 Worker 包含 1 个仅负责编排的 Team Leader 和 3 个业务 Worker；不要把包含 `accessibleTeams` 的 Human 放在 Team 之前。

部署后，进入 AgentTeams 的 Element/Matrix 管理入口，向 `goai-manager` 提交任务，例如：

```text
请读取本次 ECU 环境测试的 A2L 文件，按测试配置生成信号目录并完成指定报文测量，
先由数据处理专家 Agent 生成 Excel 原始数据（这是中间产物，不是最终报告）；再由合规流程 Agent 解析试验大纲和流程手册；
最后由报告 Agent 根据试验大纲、流程手册生成的合规模型和报告规范制作正式试验报告，并返回每个 ECU 的最大值、最小值、平均值、
需求覆盖、缺失信号、异常样本和可复核的执行证据。
```

## MCP 接入

Worker 不持有真实 API Key 或企业系统凭据。MCP Server 应挂在 Higress 后面，使用 AgentTeams 为 Worker 分配的 Consumer Token。工具名称、参数 Schema、权限、幂等和失败处理见 [mcp-contract.yaml](mcp-contract.yaml)。

如需先验证 Demiurge 的本地 stdio MCP 通道，可参考 [开源发布与免费 MCP smoke test](../../docs/OPEN-SOURCE-RELEASE.md) 中的 DuckDuckGo 配置。该测试 server 是第三方可选依赖，默认关闭，不替代 ECU 工具 MCP，也不应被描述为 DuckDuckGo 官方服务。

本地演示也提供 demo/mcp_server.py，只暴露 A2L 预筛选、合成 MDA 回放和 XLSX 写入三个受限工具；它是可替换的业务适配层，不是生产 MDA 驱动。

## 资源说明

- `agentteams-goai.yaml`：Manager、4 个 Worker、1 个 Team、1 个 Human 的声明式资源；
- `mcp-contract.yaml`：ECU/A2L/MDA/Excel 工具契约；
- `../../packs/default/skills/goai-*`：可分发到 Worker 的通用 Skill（包含三个业务 Agent Skill 与底层复用 Skill）；
- `../../docs/GOAI-ADAPTATION.md`：赛事技术要求和 Agent Identity 映射。

这里不提交任何 API Key、Matrix 密码、MDA 工具凭据或企业内网地址。
