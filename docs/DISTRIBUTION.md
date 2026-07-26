# Windows 安装包与自动更新方案

> 状态（2026-07-26）：架构建议，尚未接入 updater。当前 `tauri.conf.json` 已启用 NSIS bundle，WebView2 使用 `downloadBootstrapper`。

## 结论

首个 Windows 发布版继续使用 NSIS `.exe`。面向中国网络环境，正式稳定版建议改用 WebView2 `offlineInstaller`，避免用户安装时依赖微软下载链路；如需小体积，可另产在线安装版。

腾讯服务器可以作为安装包和更新源。私有 Gitea 镜像仓库本身不适合直接作为普通用户的更新端点：私有 release 附件和元数据需要鉴权，而写死在客户端里的 Gitea token 可被提取，无法保密。推荐二选一：

1. 在同一服务器用 Nginx/对象存储暴露只读、匿名 HTTPS 下载目录（推荐）；源代码 Gitea 仓库继续私有。
2. 建一个不含源码的、公开只读的 Gitea“二进制发布仓库”，只放 release 附件与更新 manifest。

用户不需要看到或访问源码仓库；客户端只需要匿名读取签名后的安装包、更新包和 manifest。若业务必须限制下载，应由用户登录后取得短期、可撤销的下载凭证，不能在安装包里内置长期仓库 token。

## 推荐发布拓扑

```text
私有 GitHub/Gitea 源码仓库
  └─ CI 构建、测试、签名
       └─ 上传腾讯服务器的临时版本目录
            ├─ Demiurge_<version>_x64-setup.exe       # 人工下载安装
            ├─ Demiurge_<version>_x64-setup.nsis.zip # Tauri 更新包
            ├─ Demiurge_<version>_x64-setup.nsis.zip.sig
            └─ latest.json                            # 最后发布

客户端 updater ──HTTPS──> https://download.example.com/demiurge/stable/latest.json
```

上传时先放带版本号的不可变文件，校验大小和哈希后再原子替换 `latest.json`。这样客户端永远不会看到“manifest 已更新但文件尚未上传”的半发布状态。至少保留最近两个版本，便于回滚。

## Tauri 2 更新链路

实施时需要：

- 加入 `tauri-plugin-updater`（通常配合 `tauri-plugin-process` 完成重启）并授予最小 capability。
- 在 bundle 配置启用 `createUpdaterArtifacts`，设置 updater 公钥和稳定 HTTPS endpoint。
- 用离线保存的 Tauri updater 私钥为更新包生成 `.sig`；私钥只进入 CI secret，不能进仓库或服务器公开目录。
- 客户端检查 `latest.json`，展示版本、更新说明和下载进度；下载后先验签，再安装并重启。
- 发布流水线同步更新应用版本，完成 `npm test`、`npm run build`、`cargo fmt --check`、`cargo test` 后才允许上传。

示意 manifest（字段以接入时锁定的 Tauri 插件版本为准）：

```json
{
  "version": "0.2.0",
  "notes": "本次更新说明",
  "pub_date": "2026-07-26T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<.sig 文件内容>",
      "url": "https://download.example.com/demiurge/stable/0.2.0/Demiurge_0.2.0_x64-setup.nsis.zip"
    }
  }
}
```

## 两类签名不可混为一谈

- **Tauri updater 签名**：客户端验证更新包未被篡改，是自动更新的硬安全边界。
- **Windows Authenticode**：减少 SmartScreen/“未知发布者”警告，确认发布者身份。正式外部分发强烈建议购买 OV/EV 代码签名证书或使用合规的云签名服务，并给产物加可信时间戳。

只做 HTTPS 或只附 SHA-256 都不能替代 updater 签名；只有 updater 签名也不能消除 Windows 的未知发布者提示。

## 发布前必须确认

1. 下载域名与 TLS 证书，以及是否允许匿名读取。
2. Windows Authenticode 证书/云签名方案。
3. Cubism Core 的最终用户分发许可。当前仓库不分发该私有运行时；若安装包也不能携带，需要实现首次使用时的许可确认与官方来源下载，否则安装版 Live2D 不会开箱即用。
4. 稳定/测试更新通道是否分离，以及自动下载还是用户确认后安装。
