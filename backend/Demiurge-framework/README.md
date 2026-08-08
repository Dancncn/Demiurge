# Framework

基础设施 Adapter crate，目前包含：

- `persistence/atomic_file`：原子写入和 `.bak` 恢复。
- `persistence/session`、`persistence/settings`：磁盘格式与敏感字段处理。
- `remote/webdav`：认证、集合管理、上传、列举、删除及 XML 解析。

Adapter 应实现 `core` 定义的接口。禁止让 `core` 依赖这里的具体实现。
不存在的 MQ、数据库或远程供应商不创建空目录和空类。
