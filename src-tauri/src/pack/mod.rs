//! 角色包（pack）模块：清单读写、lorebook 召回索引、Live2D 导入、包内文件浏览。
//!
//! 本 `mod.rs` 仅做子模块声明与公共 API 重导出，实际实现分布在：
//! - [`manifest`]：manifest.json 类型定义、读写校验、zip 导入解压、persona 渲染、markdown/lore 分块纯函数。
//! - [`lorebook`]：BM25 稀疏检索 + 向量稠密检索 + RRF 混合融合的召回与索引。
//! - [`live2d`]：Live2D 模型导入与文件名归一化（ASCII 化以规避 asset 协议 CJK 编码 bug）。
//! - [`files`]：包内文件浏览、lore 批量导入、素材授权警告。
//!
//! 对外公共 API 完全不变：所有 `pack::*` 调用路径仍可用（`pack::list_packs`、
//! `pack::import_zip`、`pack::lorebook_recall_detail`、`pack::import_live2d_folder`、
//! `pack::read_pack_file` 等）。
mod manifest;
mod lorebook;
mod live2d;
mod files;

pub use files::*;
pub use live2d::*;
pub use lorebook::*;
pub use manifest::*;
