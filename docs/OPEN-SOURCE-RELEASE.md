# 开源发布检查说明

本文件记录 `goai` 分支当前适合公开前需要确认的事项。它不代表真实 AgentTeams 集群、企业 MCP 或 ECU/MDA 环境已经部署。

## 当前审计结论

- 当前仓库未发现可直接使用的 API Key、Token、私钥、企业内网地址或真实 ECU 测试文件。
- 历史冒烟报告中的个人本机路径、个人音色模型名、Live2D 素材目录和另一个本地项目路径已经泛化为占位符。
- `.env`、本地 TTS 配置、临时目录、构建产物和本机运行时文件由 `.gitignore` 排除。
- `LICENSE` 和 `package.json` 中的版权署名为 `DanArnoux`；公开发布前请确认这是希望使用的公开署名。

## 免费 MCP smoke test

项目的第一阶段 MCP 客户端只支持本地 stdio server。可以手动配置下面的可选测试项；它默认关闭，不会改变项目的生产工具集合：

```json
{
  "name": "duckduckgo-search",
  "enabled": false,
  "transport": "stdio",
  "command": "uvx",
  "args": ["duckduckgo-mcp-server", "--search-backend", "auto"],
  "env": []
}
```

该 server 是社区维护的第三方实现，不是 DuckDuckGo 官方 MCP。它不需要 API Key，但会受到 DuckDuckGo 的限流、反爬和页面结构变化影响；搜索结果属于不受信外部内容，不能被当作系统指令执行。上游项目和许可证见 [nickclyde/duckduckgo-mcp-server](https://github.com/nickclyde/duckduckgo-mcp-server)。

2026-08-14 的隔离 smoke test 已完成：MCP `initialize`、`tools/list` 和 `tools/call(search)` 均成功，返回 3 条 DuckDuckGo 结果。另一个 `@oevortex/ddg_search` 候选的 MCP 握手成功，但搜索请求被 DuckDuckGo 返回 HTTP 202 拦截，因此不作为默认测试方案；其仓库和许可证见 [OEvortex/ddg_search](https://github.com/OEvortex/ddg_search)。

## 发布前人工确认

1. 检查完整 Git 历史，而不只是当前工作树；如果历史中曾提交过密钥，先吊销并清理历史。
2. 确认 `DanArnoux` 版权署名、项目名称和第三方素材的授权范围。
3. 不要提交真实 A2L、MDF/MF4、试验大纲、流程手册、企业报告模板或 ECU 测试结果；样例只能使用虚构数据和占位 URI。
4. 不要把真实 Higress 地址、Matrix 凭据、MinIO 地址、企业系统地址或 MCP Consumer Token 写入 AgentTeams 清单。
5. 对可选外部 MCP 采用最小权限、默认关闭，并在 README 中保留第三方来源和许可证说明。
