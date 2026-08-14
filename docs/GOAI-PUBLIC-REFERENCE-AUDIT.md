# GOAI 公开资料与场景边界审查

本文件记录为初赛选题收集的公开资料。它只用于证明场景和技术路线具有公开可理解的基础，不代表项目已经取得任何企业授权、硬件认证或标准符合性。

## 公开资料

| 资料 | 可公开确认的事实 | 在作品中的用途 | 边界 |
| --- | --- | --- | --- |
| ASAM MCD-2 MC / ASAP2 | 定义 ECU 测量和标定数据交换格式，覆盖 A2L/AML、变量描述、数据类型、布局、单位和换算等内容 | 说明 A2L 预筛选、解析信号目录和变量解释的技术合理性 | 只实现演示级解析；不宣称通过 ASAM A2L-Checker |
| ASAM MDF | 用于记录或计算测量数据，支持 ECU 变量、总线数据和 MF4 归档 | 说明后续 MDF/MF4 读取适配器和大数据后处理方向 | 不提交任何真实 MDF/MF4；版本和兼容性在接入前重新核对 |
| 同星 TSMaster | 官方公开资料介绍其汽车总线连接、监控、仿真、诊断、标定、测试测量和 BLF/ASC/MAT 数据路径 | 作为“可替换 CAN/MDA 采集或回放适配器”的公开场景背景 | 不宣称官方合作、真实硬件已接通或获得 TSMaster 内部接口授权 |
| ISO 16750-4 / GB/T 28046.4 | 道路车辆电气电子设备的气候负荷和环境条件测试语境 | 说明车载 ECU 温度、湿热等环境测试具备行业普遍性 | 不复制标准正文；具体试验条件必须来自授权的试验大纲 |
| IEC 60068-2-14 | 温度变化测试方法的公开标准条目 | 支撑“温度变化/温度冲击”作为模拟测试类型 | 不把示例温度、循环数或持续时间说成标准规定值 |
| IEC 60068-2-30 | 高湿、循环温度变化和通常会产生结露的环境测试方法 | 支撑“湿热循环/结露”作为模拟测试类型 | 不把合成条件说成产品验收条件 |
| AgentTeams 官方仓库 | Manager、Worker、Team、Human 等资源与可见协作、网关、存储和控制器架构 | 作为多 Agent 设计基点和部署映射依据 | 当前仓库仅有适配清单和本地模拟入口，真实集群仍需部署验证 |
| DeepSeek 官方文档 | 提供 OpenAI 兼容的 Chat Completions 调用方式 | 作为可选 LLM 复核环节 | API Key 不进入仓库；费用、余额、速率和模型可用性需由运行者承担 |

## 官方或权威链接

- ASAM MCD-2 MC：https://www.asam.net/standards/detail/mcd-2-mc/
- ASAM MDF：https://www.asam.net/standards/detail/mdf/
- 同星 TSMaster：https://www.tosunai.com/product/tsmaster/
- ISO 16750-4:2023：https://www.iso.org/standard/77580.html
- GB/T 28046.4-2011：https://std.samr.gov.cn/gb/search/gbDetailed?id=71F772D8005FD3A7E05397BE0A0AB82A
- IEC 60068-2-14:2023：https://webstore.iec.ch/en/publication/71503
- IEC 60068-2-30:2025：https://webstore.iec.ch/en/publication/82356
- AgentTeams 架构：https://github.com/agentscope-ai/AgentTeams/blob/main/docs/architecture.md
- DeepSeek API：https://api-docs.deepseek.com/

## 复用项目与依赖判断

通用 Excel MCP 可以考虑 [negokaz/excel-mcp-server](https://github.com/negokaz/excel-mcp-server) 或 [haris-musa/excel-mcp-server](https://github.com/haris-musa/excel-mcp-server)。本项目当前没有把它们作为强依赖，因为初赛模拟包需要离线、零第三方 Python 依赖，并且只允许向受控目录写入固定格式的原始数据。后续真实部署可以将本地 Excel 适配器替换为锁定版本的社区 MCP，并在接入前完成许可证、路径隔离和权限审查。

MDF/MF4 后续可以评估 [asammdf](https://github.com/danielhrisca/asammdf)。它是测量数据处理库，不是完整业务 MCP；项目仍需要自行定义输入输出、证据、失败处理和权限边界。

## 提交前核验

1. 重新打开官方赛事页面和参赛手册，确认提交时间、字数和代码包要求没有更新。
2. 对标准版本、TSMaster 功能描述和 AgentTeams 版本做一次最终核验。
3. 不使用未经授权的企业 A2L、MDF/MF4、CAN 日志、试验大纲、报告模板或截图。
4. 所有作品材料明确标注合成数据和演示边界。
5. 对商业 API、闭源模型、第三方 MCP、许可证、费用和迁移成本做披露。
