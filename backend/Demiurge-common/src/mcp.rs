use serde::{Deserialize, Serialize};

fn default_enabled() -> bool {
    true
}

fn default_transport() -> McpTransportKind {
    McpTransportKind::Stdio
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportKind {
    Stdio,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpEnvVar {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_transport")]
    pub transport: McpTransportKind,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<McpEnvVar>,
}

impl McpServerConfig {
    pub fn normalized_name(&self) -> String {
        normalize_segment(&self.name, 32)
    }

    pub fn signature(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

pub fn normalize_segment(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in value.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            ch
        } else {
            '_'
        };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "server".to_string()
    } else {
        out
    }
}
