# TypeScript 7 编译提速可行性

评估日期：2026-10-02（Asia/Shanghai）。结论：本项目适合单独试迁原生类型检查器，但不能原样升级配置。本次固定源码快照中，类型检查中位数从 **5.222 秒降到 0.760 秒，约 6.88 倍**；“类型检查 + Vite 打包”的构建有效负载从 **19.904 秒降到 15.339 秒，节省 22.9%**。Vite 已占主要耗时，不能把类型检查的倍数直接套到完整构建。

TS7 评估只增加报告，不改编译配置。仓库的 package.json、package-lock.json、tsconfig 和默认构建命令均未升级；候选包、配置、源码快照和输出位于操作系统临时目录。

## 官方发布状态与候选版本

微软在 **2026-07-08** 发布 TypeScript 7.0；原生版本使用 Go 实现，正式包重新使用 `typescript` 与 `tsc` 名称。官方明确 TS7 与 TS6 的检查语义对齐，并移除了 TS6 弃用的部分配置，不能假定 TS5 配置无需迁移。[官方发布说明](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)

直接查询 npm registry 得到以下状态，时间与用户日期 2026-10-02 一致，没有把 nightly 当成稳定版：

| 包或 tag | 查询到的精确版本 | 发布/更新时间（UTC） | 本次用途 |
| --- | --- | --- | --- |
| `typescript@latest` | `7.0.2` | 版本发布：2026-07-08T15:55:18.431Z | 主要候选，原生命令 `tsc` |
| `typescript@next` | `7.1.0-dev.20261002.1` | 包 metadata modified：2026-10-02T08:23:56.111Z | 仅记录，未安装和测量 |
| `@typescript/native-preview@latest` | `7.0.0-dev.20260707.2` | 版本发布：2026-07-07T08:20:24.277Z | 对照旧名称 `tsgo`，不建议新迁移继续选择它 |

数据源：[TypeScript registry](https://registry.npmjs.org/typescript)、[稳定版 metadata](https://registry.npmjs.org/typescript/7.0.2)、[native-preview registry](https://registry.npmjs.org/@typescript%2fnative-preview)。表中的 `modified` 是整个包 metadata 的更新时间，不冒充 next 版本的精确发布时间。

`microsoft/typescript-go` 已回迁至原 TypeScript 仓库，页面显示于 2026-09-01 归档；README 说明 RC 起命令名为 `tsc`。旧 preview README 中“尚未实现全部功能”的段落不应单独用于判断当前稳定版成熟度。[官方仓库 README](https://github.com/microsoft/typescript-go/blob/main/README.md)、[preview 包说明](https://github.com/microsoft/typescript-go/blob/main/_packages/native-preview/README.md)

## 项目的实际检查与打包链路

[frontend/package.json](../frontend/package.json) 的构建为 `tsc --noEmit && vite build`。声明的依赖范围是 TypeScript `^5.6.3`、Vite `^6.0.3`，本机实际安装及本次使用的是 **TypeScript 5.9.3、Vite 6.4.3**。Node 为 24.14.1，npm 为 11.11.0。

[tsconfig.json](../frontend/tsconfig.json) 仅检查 `src`，启用 strict、noUnusedLocals、noUnusedParameters、isolatedModules、skipLibCheck，使用 ES2021、ESNext module、bundler resolution 和 React JSX，没有 incremental/project references，也不产出 JavaScript。测试目录与 vite.config.ts 本来就不在此检查范围内；本次没有缩小 include 或新开 skipLibCheck。

[vite.config.ts](../frontend/vite.config.ts) 独立执行 React、Tailwind、静态资源和 Rollup chunk 打包；本次每次仍处理 3417 modules。Vite 6 的官方文档也区分转译与类型检查，推荐把 `tsc --noEmit` 作为独立构建步骤。[Vite 6 TypeScript 文档](https://v6.vite.dev/guide/features#typescript)

因此 TS7 主要加速前置类型检查。现有 Node 原生 strip-types 测试、Vite 开发服务、应用运行和 Rust 编译不会因替换这个检查器直接获得同样收益。当前工程未发现需要调用 TypeScript Compiler API 的自有构建插件；未来加入此类工具时仍须另验，不能把 TS7 CLI 兼容当成 TS5 Compiler API 兼容。稳定包暴露的是 `unstable/*` API，而非旧版 API 的原样替代。[稳定包 exports](https://registry.npmjs.org/typescript/7.0.2)

## 配置兼容性实测

| 配置 | TS5.9.3 | TS7.0.2 | 旧 tsgo preview |
| --- | --- | --- | --- |
| 原始 tsconfig | 通过 | 失败：TS5102 baseUrl 已删除，TS5090 paths 需相对前缀 | 同样失败 |
| 仅去掉 baseUrl，`src/*` 改为 `./src/*` | 未作为计时配置 | 失败：两个字体包的副作用导入缺声明（TS2882） | 未单独运行此变体 |
| 下述共同计时配置 | 通过 | 通过 | 通过 |

字体诊断位于快照 `src/main.tsx` 的 `@fontsource-variable/inter` 和 `@fontsource-variable/jetbrains-mono` 导入。原因是 TS7 默认开启 `noUncheckedSideEffectImports`。新版本对默认 `types` 和部分配置也有变化，详见[官方迁移说明](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#updates-since-5-x-and-new-behaviors-from-6-0)。

共同计时配置在临时快照中从原配置生成，只做以下调整，三种编译器使用同一个文件：

```text
移除 compilerOptions.baseUrl
paths["@/*"] = ["./src/*"]
types = 快照所用 node_modules/@types 下按名称排序的全部 49 个包
noUncheckedSideEffectImports = false
libReplacement = true
其余 compilerOptions 和 include 保持原值
```

显式 `types`、副作用导入选项和 libReplacement 用于保留 TS5 原来隐含的检查环境，避免以检查更少文件获取速度收益。TS5 原配置与共同配置的 `--listFilesOnly` 输出完全相同；三种共同配置均列出 **1175 个输入文件，其中项目 src 文件 67 个、@types 文件 53 个**。各编译器自带标准库的版本/内容仍会不同，不能据此声称类型系统完全等价。

49 个包的显式列表只是基准测试固定输入的方法，不建议把所有传递依赖名称原样固化成长期配置。未来正式迁移可明确真正需要的全局类型，并为两个字体导入补准确声明；也可以显式保留现有副作用导入检查语义。任一选择都应单独审阅，不能把这次兼容性通过当成已合并升级。

## 测量方法与环境

- 固定提交：`9c72d3f4214d13eb5f2152f87ef8eb44acebf3ac`。用 `git archive` 导出 frontend、根 package.json 和 package-lock.json，避免其他 agent 同时修改前端导致比较不同代码。
- Windows 11 Pro for Workstations 10.0.26200；系统报告 CPU 为 Intel i9-13900HX、24 cores/24 logical processors，可见内存约 32 GiB。没有锁定电源频率、关闭系统服务或测量峰值内存。
- 源码快照在 C 盘用户临时目录，通过 junction 使用仓库 D 盘同一份 node_modules；候选包单独安装在临时目录，安装时使用精确版本、`--ignore-scripts --no-audit --no-fund`。
- 测量窗口为上海时间 **19:23:13–19:26:23**，协调暂停其他 agent 的 Cargo/npm/Vite 构建。编辑可继续，但不影响快照。不能排除操作系统和杀毒软件的后台活动。
- 每种编译器预热 1 次，不纳入统计；随后每轮交替次序，共 6 轮。每次启动全新进程，使用相同共同配置，无增量缓存、无 watch、无 `.tsbuildinfo`；保留操作系统文件缓存。这是“暖文件缓存下的完整检查”，不是冷启动基准。
- 为包含正常 CLI 启动成本，统一用同一个 Node 执行各包的 bin wrapper，未给原生版本绕过 wrapper 的特殊捷径。stdout/stderr 均写入日志，计时使用 `process.hrtime.bigint()`。
- 完整前端构建对照各运行 3 次，执行共同配置检查后运行同一个 Vite build。计时包含 CLI 启动、转译、打包、资源输出和压缩体积报告；不包含两层 npm script 启动开销、依赖安装、Tauri/Rust 或安装包制作。本文“构建有效负载”特指这一范围。

## 原始类型检查计时

单位为毫秒，所有计时检查退出码均为 0。A=TS5.9.3、B=TS7.0.2、C=tsgo preview 7.0.0-dev.20260707.2。

| 轮次 | 执行次序 | A | B | C |
| --- | --- | ---: | ---: | ---: |
| 1 | A → B → C | 5311.4 | 793.5 | 704.0 |
| 2 | C → B → A | 5132.8 | 806.0 | 809.8 |
| 3 | B → C → A | 5902.5 | 781.6 | 726.4 |
| 4 | A → C → B | 5322.1 | 737.4 | 798.0 |
| 5 | B → A → C | 4992.3 | 699.8 | 662.7 |
| 6 | C → A → B | 4440.0 | 646.8 | 482.1 |
| **中位数** | | **5222.1** | **759.5** | **715.2** |

TS7 的类型检查耗时减少约 **85.5%**。旧 preview 稍快不能证明它比稳定版更值得采用；这里样本很小，差异与测量波动相近。

原配置 TS5 的 3 次补充对照是 **5075.5、4936.8、5130.8 ms**，中位数 **5075.5 ms**。共同配置中位数比原配置高约 2.9%，并没有通过简化检查范围提速。预热记录：TS5 6277.5 ms、TS7 743.8 ms、tsgo 611.2 ms；预热不应与后续结果混合计算。

## 原始构建计时与收益上限

单位为毫秒。第 1、3 组先 TS5 后 TS7，第 2 组顺序相反。每个组内的 Vite 参数和源码完全相同；总耗时另包含记录日志的少量开销。

| 组 | 检查器 | 类型检查 | Vite | 合计 |
| --- | --- | ---: | ---: | ---: |
| 1 | TS5 | 5430.2 | 13835.6 | 19269.6 |
| 1 | TS7 | 531.6 | 14080.2 | 14615.8 |
| 2 | TS5 | 4903.6 | 14995.5 | 19904.4 |
| 2 | TS7 | 741.5 | 15839.5 | 16584.6 |
| 3 | TS5 | 6697.5 | 16011.8 | 22715.5 |
| 3 | TS7 | 812.7 | 14522.0 | 15339.4 |
| **中位数** | **TS5** | **5430.2** | **14995.5** | **19904.4** |
| **中位数** | **TS7** | **741.5** | **14522.0** | **15339.4** |

Vite 预热为 16475.5 ms，不纳入表内统计。各次仍有约 1106.38 kB 的 Live2D vendor chunk 提示，gzip 307.77 kB；检查器更换不解决该资源体积。

完整有效负载实测中位数节省 **4.565 秒 / 22.9%**，约 **1.30 倍**。Vite 两列的差异是相同工作负载的波动，不能解释为 TS7 加速了 Vite。

用原构建各阶段中位数构造简单模型：`T=5.430s`、`V=14.996s`，Vite 约占 **73.4%**。即使把类型检查缩短到零，前端有效负载也只能从 `T+V=20.426s` 降到 `V=14.996s`，理想上限约 **26.6% 节省 / 1.36 倍**。把 TS7 的 `0.742s` 代入，模型预测约 **23.0% 节省**，与观测一致。这里“阶段中位数之和”只是收益模型，不等于表内“合计的中位数”。

真正 `npm run build` 还有 npm wrapper 开销，Tauri build 还有 Rust 与安装包阶段，因此只替换 TypeScript 对那些总流程的收益上限会更低。这是本机、该快照、暖缓存条件下的估算，不是所有电脑上的时间承诺。

## 后续采用建议与未验证项

1. 若要升级，优先试 **稳定 `typescript@7.0.2`**，而非停留在旧 tsgo preview；先保留 TS5 作为对照检查，在一项独立变更中迁移配置和字体声明。
2. 正式迁移时仍应对当时的工作区运行类型检查和前端行为测试。下文补充的复核已确认本轮优化后源码通过 TS7 类型检查；上面的性能数字仍只对应固定提交快照，不能当作优化后源码的重新计时结果。
3. 继续保留 strict、unused 检查和当前 src include；不要通过关闭检查来兑现速度目标。`skipLibCheck` 原先就为 true，这次没有把依赖声明的检查从开启改为关闭。
4. 若将来需要编译器 API、编辑器插件、watch、JS/JSDoc 输入或 `.d.ts` emit，分别做兼容验证；此次 noEmit CLI 成功不包含这些承诺。官方也记录了有意改变的检查/JS 行为。[官方 CHANGES](https://github.com/microsoft/typescript-go/blob/main/CHANGES.md)
5. 没有实测冷文件缓存、增量构建、CI 虚拟机、内存峰值、编辑器体验或 Tauri 安装包耗时；不要把本报告用于这些场景的定量预测。

## 复核材料

临时评估目录（不属于仓库，可能被系统清理）：

```text
C:\Users\DanArnoux\AppData\Local\Temp\demiurge-ts7-7573f83d12f24ca6a633ca7f0c763f35
  benchmark.mjs                 # 完整计时程序、交替次序与实际 argv
  benchmark-results.json        # 未四舍五入的计时、退出码、时间和哈希
  logs/                         # 各次检查、构建、输入文件清单及失败诊断
  snapshot/                     # git archive 固定源码与临时配置
  candidate/                    # 精确版本候选包与隔离 package-lock
  typescript-registry.json      # 本次 registry 原始响应
  native-preview-registry.json
```

复做时先 `git archive` 同一提交，使用同一锁文件安装依赖，在快照根目录解析同一 node_modules；根据上面的配置差异生成共同 tsconfig。对三种包的 bin wrapper 使用相同 Node，运行 `--noEmit --project tsconfig.bench.json --pretty false`，先各预热一次，再按表内顺序交替；打包接同一 Vite 6.4.3 的 `bin/vite.js build`。不要在日后复测中直接使用浮动 latest/next，也不要混入同时运行的 Cargo 或其他前端构建。

关键 SHA-256：

```text
原 tsconfig.json   f9afca3f82efae245b26c7bd5d9045195c381c23a90b2c6f7a1ab59e820be155
共同 tsconfig      9df553acdaa6f2fb3bf83031843267184240356633a52c3ea7116e7b44678ed6
vite.config.ts     6b08c4b5d6064c626a6d56566fdf7e8a5b9126b34599b9afa3334344faa3c12b
package-lock.json  291b01589549b180242de434d8129f37694e32761063c4b1fcf017024e09aae2
```

## 优化后工作区兼容性复核

**2026-10-02 19:46（Asia/Shanghai）**，使用上述隔离安装的稳定 **TypeScript 7.0.2**，对当前 `D:/Project/Project-1/Demiurge/frontend/src` 执行一次 `--noEmit --pretty false --listFiles`，**退出码为 0，无类型诊断**。检查前后全部 src 文件的 SHA-256 清单一致，运行期间源码未变化。

临时配置 `tsconfig.current-workspace.json` 复用共同计时配置的所有检查选项和 49 个显式 types，仅将 include、`paths["@/*"]` 和 typeRoots 分别明确指向当前仓库的绝对 src 路径、绝对 src 别名路径及 `D:/Project/Project-1/Demiurge/node_modules/@types`。本次没有指向旧 snapshot 源码，也没有调整仓库配置、依赖或锁文件。

实际输入清单为 **1179 个文件，其中当前项目 src 文件 71 个**，明确包含本轮新增的 `PermissionSettings.tsx`、`permissionSettingsModel.ts`、`messageProjection.ts` 和 `confirmationState.ts`。配置、`current-workspace-recheck.json` 和 `logs/current-workspace-ts7-noemit.log` 保留在上述临时评估目录。

本次仅补充当前源码的 noEmit 兼容性证据，没有重新计时或运行 Vite，也没有重复 TS5 对照。它不覆盖测试目录、原生 GUI、运行时行为、编辑器或默认构建升级；上文的性能测量范围和限制保持不变。
