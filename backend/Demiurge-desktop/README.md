# Desktop

Tauri 桌面入口与用例编排 crate。

```text
src/
├─ controller/     # 117 个 Tauri IPC Adapter；只做参数/响应适配
├─ biz/            # Agent、Session、Goal、Memory、Pack、Remote 等用例编排
├─ starter/
│  └─ state.rs     # AppState 与运行态
├─ starter.rs      # Adapter 装配、恢复、窗口预创建、Command 注册
├─ lib.rs          # 27 行 crate facade
└─ agent, tools…   # 桌面专属 Service 与 Adapter 实现
```

调用方向：

```text
starter -> controller::<feature> -> biz::<feature>
                                      ├─ core
                                      ├─ framework
                                      └─ desktop service/adapter
```

Controller 不得直接访问 AppState 存储字段、文件系统、HTTP Client 或持久化。
Starter 不承载领域行为。没有第二个实现或明确替换需求时不创建空 trait。
