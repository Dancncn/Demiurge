use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct WebDavConfig {
    url: String,
    username: String,
    password: String,
    path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WebDavBackupFile {
    pub file_name: String,
    pub modified_time: String,
    pub size: u64,
}

/// Hides WebDAV authentication, collection management and XML parsing behind
/// the four operations needed by the backup use case.
pub struct WebDavRemote<'a> {
    client: &'a reqwest::Client,
}

impl<'a> WebDavRemote<'a> {
    pub fn new(client: &'a reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn check_connection(&self, config: &WebDavConfig) -> Result<(), String> {
        self.ensure_collection(config).await
    }

    pub async fn upload_json(
        &self,
        config: &WebDavConfig,
        file_name: &str,
        body: Vec<u8>,
    ) -> Result<(), String> {
        self.ensure_collection(config).await?;
        let url = file_url(config, file_name)?;
        let response = auth(self.client.put(url), config)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| format!("上传 WebDAV 备份失败：{error}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("上传 WebDAV 备份失败：HTTP {}", response.status()))
        }
    }

    pub async fn list_backups(
        &self,
        config: &WebDavConfig,
    ) -> Result<Vec<WebDavBackupFile>, String> {
        let body = self.propfind(config, true).await?;
        Ok(parse_backup_files(&body))
    }

    pub async fn delete_backup(
        &self,
        config: &WebDavConfig,
        file_name: &str,
    ) -> Result<(), String> {
        validate_backup_file_name(file_name)?;
        let url = file_url(config, file_name)?;
        let response = auth(self.client.delete(url), config)
            .send()
            .await
            .map_err(|error| format!("删除 WebDAV 备份失败：{error}"))?;
        if response.status().is_success() || response.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(format!("删除 WebDAV 备份失败：HTTP {}", response.status()))
        }
    }

    async fn propfind(&self, config: &WebDavConfig, depth_one: bool) -> Result<String, String> {
        let method = reqwest::Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?;
        let response = auth(self.client.request(method, collection_url(config)?), config)
            .header("Depth", if depth_one { "1" } else { "0" })
            .header("Content-Type", "application/xml")
            .body(
                r#"<?xml version="1.0" encoding="utf-8" ?>
<propfind xmlns="DAV:">
  <prop>
    <displayname />
    <getcontentlength />
    <getlastmodified />
    <resourcetype />
  </prop>
</propfind>"#,
            )
            .send()
            .await
            .map_err(|error| format!("WebDAV PROPFIND 失败：{error}"))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() || status.as_u16() == 207 {
            Ok(body)
        } else {
            Err(format!("WebDAV PROPFIND 失败：HTTP {status}"))
        }
    }

    async fn ensure_collection(&self, config: &WebDavConfig) -> Result<(), String> {
        if self.propfind(config, false).await.is_ok() {
            return Ok(());
        }
        let method = reqwest::Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?;
        let response = auth(self.client.request(method, collection_url(config)?), config)
            .send()
            .await
            .map_err(|error| format!("创建 WebDAV 目录失败：{error}"))?;
        if response.status().is_success() || response.status().as_u16() == 405 {
            Ok(())
        } else {
            Err(format!("创建 WebDAV 目录失败：HTTP {}", response.status()))
        }
    }
}

fn auth(request: reqwest::RequestBuilder, config: &WebDavConfig) -> reqwest::RequestBuilder {
    let username = config.username.trim();
    if username.is_empty() {
        request
    } else {
        request.basic_auth(username.to_string(), Some(config.password.clone()))
    }
}

fn collection_url(config: &WebDavConfig) -> Result<String, String> {
    let base = config.url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("WebDAV URL must start with http:// or https://.".to_string());
    }
    let path = config.path.trim().trim_matches('/');
    if path.is_empty() {
        Ok(format!("{base}/"))
    } else {
        Ok(format!("{base}/{path}/"))
    }
}

fn file_url(config: &WebDavConfig, file_name: &str) -> Result<String, String> {
    validate_backup_file_name(file_name)?;
    Ok(format!("{}{}", collection_url(config)?, file_name))
}

fn validate_backup_file_name(file_name: &str) -> Result<(), String> {
    let valid = file_name.starts_with("demiurge-backup-")
        && file_name.ends_with(".json")
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && !file_name.contains("..");
    valid
        .then_some(())
        .ok_or_else(|| "Invalid backup file name.".to_string())
}

static RESPONSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[^:>/]*:?response\b[^>]*>.*?</[^:>/]*:?response>")
        .expect("valid WebDAV response regex")
});
static HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[^:>/]*:?href[^>]*>(.*?)</[^:>/]*:?href>").expect("valid href regex")
});
static MODIFIED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[^:>/]*:?getlastmodified[^>]*>(.*?)</[^:>/]*:?getlastmodified>")
        .expect("valid modified regex")
});
static SIZE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[^:>/]*:?getcontentlength[^>]*>(.*?)</[^:>/]*:?getcontentlength>")
        .expect("valid size regex")
});

fn parse_backup_files(body: &str) -> Vec<WebDavBackupFile> {
    let mut files = Vec::new();
    for response in RESPONSE_RE.find_iter(body).map(|value| value.as_str()) {
        let Some(href) = HREF_RE
            .captures(response)
            .and_then(|capture| capture.get(1))
            .map(|value| xml_unescape(value.as_str()))
        else {
            continue;
        };
        let file_name = percent_decode(href.trim_end_matches('/').rsplit('/').next().unwrap_or(""));
        if validate_backup_file_name(&file_name).is_err() {
            continue;
        }
        let modified_time = MODIFIED_RE
            .captures(response)
            .and_then(|capture| capture.get(1))
            .map(|value| xml_unescape(value.as_str()))
            .unwrap_or_default();
        let size = SIZE_RE
            .captures(response)
            .and_then(|capture| capture.get(1))
            .and_then(|value| value.as_str().trim().parse::<u64>().ok())
            .unwrap_or(0);
        files.push(WebDavBackupFile {
            file_name,
            modified_time,
            size,
        });
    }
    files.sort_by(|left, right| right.file_name.cmp(&left.file_name));
    files
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                output.push(value);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_backup_names() {
        assert!(validate_backup_file_name("demiurge-backup-10.json").is_ok());
        assert!(validate_backup_file_name("../demiurge-backup-10.json").is_err());
        assert!(validate_backup_file_name("other.json").is_err());
    }

    #[test]
    fn parses_only_valid_backup_entries() {
        let body = r#"
          <d:multistatus xmlns:d="DAV:">
            <d:response>
              <d:href>/backup/demiurge-backup-20.json</d:href>
              <d:getlastmodified>today</d:getlastmodified>
              <d:getcontentlength>42</d:getcontentlength>
            </d:response>
            <d:response><d:href>/backup/other.json</d:href></d:response>
          </d:multistatus>
        "#;
        assert_eq!(
            parse_backup_files(body),
            vec![WebDavBackupFile {
                file_name: "demiurge-backup-20.json".to_string(),
                modified_time: "today".to_string(),
                size: 42,
            }]
        );
    }
}
