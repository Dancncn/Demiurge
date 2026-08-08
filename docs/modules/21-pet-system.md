# 桌宠包系统

## 目标

桌宠是独立于角色包和 Live2D 的可导入资源。Demiurge 只实现稳定的运行时协议，不依赖任何具体角色；奶龙是首个使用该协议的普通包。

## 调用链

```text
frontend/features/pet
  -> frontend/lib/api.ts
  -> controller/pet.rs
  -> biz/pet.rs
  -> desktop/pet/mod.rs
```

设置中的 `current_pet` 只保存桌宠 ID，与 `current_pack` 无关。桌宠目录位于应用数据目录的 `pets/<id>`。

## `.demipet` 格式

`.demipet` 是 ZIP 容器，包根目录或唯一一级子目录中必须包含 `pet.json`。当前 schema 版本为 `1`，渲染器为声明式 `sprite-sheet`：

```json
{
  "schemaVersion": 1,
  "id": "sample-pet",
  "name": "Sample Pet",
  "version": "1.0.0",
  "thumbnail": "thumbnail.png",
  "renderer": {
    "type": "sprite-sheet",
    "sheet": "spritesheet.webp",
    "frameWidth": 192,
    "frameHeight": 208,
    "columns": 8
  },
  "actions": {
    "idle": {
      "frames": { "row": 0, "start": 0, "count": 8 },
      "frameDurationMs": 140,
      "mode": "loop"
    }
  },
  "bindings": {
    "system.idle": { "action": "idle" }
  }
}
```

动作模式支持 `loop`、`once` 和 `hold`。`once` 可通过 `returnTo` 声明 `previous`、`idle` 或具体动作。运行时语义绑定包括 `system.idle`、`agent.thinking`、`agent.working`、`agent.reviewing`、`agent.success`、`agent.failure`、`motion.left`、`motion.right`、`interaction.hover`、`interaction.tap`。拖动桌宠窗口时按起始位移方向触发左右移动动作，拖动结束后恢复之前的交互或 Agent 状态。声音条目可声明 `"loop": true`，动作切换或悬停结束时运行时会立即停止循环音频。

## 安全边界

桌宠包不执行包内代码。导入器限制压缩包大小、解压文件数、清单大小和资源扩展名；拒绝绝对路径、父目录跳转、符号链接和安装目录外资源；新版本先在临时目录完整校验，再事务替换旧版本。

## 兼容性

现有角色包、Live2D 和桌面陪伴设置键保持不变。旧设置缺少 `current_pet` 时保持未选择状态；内置包会出现在桌宠列表中，但通用设置层不引用任何具体角色 ID。
