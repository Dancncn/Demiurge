# Demiurge Frontend

React、TypeScript 与 Vite 前端。该目录拥有自己的构建配置、运行时公开资源和前端测试。

源码按装配、功能域、共享视图与跨域基础能力组织：

- `src/app`：主应用编排和导航壳。
- `src/features`：agent、chat、dashboard、workspace、workflow、voice、companion、live2d、media、pack、settings 等实际功能。
- `src/shared/components`：不拥有业务状态的通用视图组件。
- `src/lib`：稳定 IPC facade、共享契约和跨功能基础能力。
- `src/main.tsx`：根据 Tauri window label 选择应用壳，不承载业务行为。

禁止前端直接了解后端持久化对象或第三方 Provider 的具体实现。
功能代码使用 `@/` 绝对导入，移动模块时不依赖目录深度；已删除旧的通用
`src/components` 聚合目录，防止所有功能再次堆回同一层。
