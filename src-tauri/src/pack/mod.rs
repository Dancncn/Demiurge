//! 角色包加载 + 清单。MVP 文本版清单，格式预留可成长字段（Live2D / TTS / 表情等）。
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackManifest {
    pub id: String,
    pub name: String,
    /// persona 文件名（相对包目录）
    pub persona: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

pub struct Pack {
    // 预留：后续 avatar / 名称 / Live2D 等字段会从这里读取
    #[allow(dead_code)]
    pub manifest: PackManifest,
    pub persona_text: String,
}

/// 内置的通用人格（通用、不绑定任何特定角色）。首启动时落地为 packs/default。
const DEFAULT_MANIFEST: &str = r#"{
  "id": "default",
  "name": "Demiurge",
  "persona": "persona.md"
}
"#;
const DEFAULT_PERSONA: &str = r#"你是用户的桌面伴侣。性格温和、好奇、乐于助人。
你会用自然、口语化的方式陪用户聊天，也能在需要时调用工具帮用户查信息、整理文件、打开网页等。
说话简洁、不绕弯、不过度客套。遇到不确定的事会如实说不知道，而不是编造。
"#;

/// 确保 packs 目录存在，且至少有一个可用的 default 包。
pub fn ensure_default(packs_dir: &Path) -> Result<(), String> {
    let dir = packs_dir.join("default");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let manifest = dir.join("manifest.json");
    if !manifest.exists() {
        fs::write(&manifest, DEFAULT_MANIFEST).map_err(|e| e.to_string())?;
    }
    let persona = dir.join("persona.md");
    if !persona.exists() {
        fs::write(&persona, DEFAULT_PERSONA).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 列出 packs 目录下所有含 manifest.json 的子目录。
pub fn list_packs(packs_dir: &Path) -> Vec<PackManifest> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(packs_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let mf = p.join("manifest.json");
        if let Ok(txt) = fs::read_to_string(&mf) {
            if let Ok(m) = serde_json::from_str::<PackManifest>(&txt) {
                out.push(m);
            }
        }
    }
    out
}

fn pack_dir(packs_dir: &Path, id: &str) -> PathBuf {
    packs_dir.join(id)
}

/// 按 id 加载角色包（读 manifest + persona 正文）。
pub fn load_pack(packs_dir: &Path, id: &str) -> Result<Pack, String> {
    let dir = pack_dir(packs_dir, id);
    let mf = dir.join("manifest.json");
    let txt = fs::read_to_string(&mf).map_err(|e| format!("读取角色包 {id} 清单失败：{e}"))?;
    let manifest: PackManifest =
        serde_json::from_str(&txt).map_err(|e| format!("解析角色包 {id} 清单失败：{e}"))?;
    let persona_path = dir.join(&manifest.persona);
    let persona_text =
        fs::read_to_string(&persona_path).map_err(|e| format!("读取 persona 失败：{e}"))?;
    Ok(Pack {
        manifest,
        persona_text,
    })
}
