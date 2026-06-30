# Demiurge 默认角色扩展设定

这里可以放角色背景、世界观、长期陪伴设定、台词样例和可检索 lore。
核心人格、说话风格和 OOC 规则应优先写入 manifest.json 或 persona.md；长篇剧情文本适合放在 lore/ 中，由本地 Lorebook 检索按需注入上下文。

Markdown 文件可选 frontmatter：

```yaml
---
title: 示例设定
tags: [世界观, 剧情]
keywords: [关键地点, 关键事件]
priority: 0.5
---
```

`manifest.json` 中的 `lorebook.path` 可以指向单个文件，也可以指向目录：

```json
{
  "path": "lore",
  "title": "角色扩展设定",
  "tags": ["world", "plot"],
  "recursive": true,
  "extensions": ["md", "txt"],
  "priority": 0.5
}
```
