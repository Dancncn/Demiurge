//! MCP client runtime for stdio servers.
//!
//! The first slice intentionally keeps transport scope narrow: local stdio
//! servers, dynamic tool discovery, resource listing/reading, and tool calls
//! through Demiurge's existing permission gate.

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::oneshot;

use crate::store;
use crate::tools::{
    PermissionEffect, PermissionPolicy, PermissionScope, ToolConcurrency, ToolDefinition,
    ToolOutputPolicy, ToolRisk,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const TOOL_TIMEOUT: Duration = Duration::from_secs(600);
const STDERR_CAP: usize = 256 * 1024;
const RESULT_CAP_CHARS: usize = 100_000;

type PendingSender = oneshot::Sender<Result<Value, String>>;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    Disabled,
    Pending,
    Connected,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpToolView {
    pub name: String,
    pub server_name: String,
    pub original_name: String,
    pub title: Option<String>,
    pub description: String,
    pub risk: ToolRisk,
    pub read_only: bool,
    pub destructive: bool,
    pub open_world: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpResourceView {
    pub uri: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpServerView {
    pub name: String,
    pub enabled: bool,
    pub transport: McpTransportKind,
    pub command: String,
    pub args: Vec<String>,
    pub status: McpServerStatus,
    pub error: Option<String>,
    pub server_info: Option<String>,
    pub instructions: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub updated_at: u64,
    pub stderr_tail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpPanelState {
    pub servers: Vec<McpServerView>,
    pub tools: Vec<McpToolView>,
    pub resources: HashMap<String, Vec<McpResourceView>>,
}

#[derive(Clone, Debug)]
struct McpAnnotations {
    title: Option<String>,
    read_only: bool,
    destructive: bool,
    open_world: bool,
}

#[derive(Clone, Debug)]
struct McpTool {
    exposed_name: String,
    server_name: String,
    original_name: String,
    description: String,
    input_schema: Value,
    annotations: McpAnnotations,
}

#[derive(Clone, Debug)]
struct McpResource {
    uri: String,
    name: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
}

#[derive(Clone)]
struct McpClientHandle {
    writer: Arc<tokio::sync::Mutex<ChildStdin>>,
    child: Arc<tokio::sync::Mutex<Child>>,
    pending: Arc<Mutex<HashMap<u64, PendingSender>>>,
    next_id: Arc<AtomicU64>,
    stderr: Arc<Mutex<String>>,
}

struct McpServerRuntime {
    config: McpServerConfig,
    signature: String,
    status: McpServerStatus,
    error: Option<String>,
    server_info: Option<String>,
    instructions: Option<String>,
    capabilities: Value,
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
    handle: Option<McpClientHandle>,
    updated_at: u64,
}

impl McpServerRuntime {
    fn disabled(config: McpServerConfig) -> Self {
        McpServerRuntime {
            signature: config.signature(),
            config,
            status: McpServerStatus::Disabled,
            error: None,
            server_info: None,
            instructions: None,
            capabilities: json!({}),
            tools: Vec::new(),
            resources: Vec::new(),
            handle: None,
            updated_at: store::now_millis(),
        }
    }

    fn failed(config: McpServerConfig, error: String) -> Self {
        McpServerRuntime {
            signature: config.signature(),
            config,
            status: McpServerStatus::Failed,
            error: Some(error),
            server_info: None,
            instructions: None,
            capabilities: json!({}),
            tools: Vec::new(),
            resources: Vec::new(),
            handle: None,
            updated_at: store::now_millis(),
        }
    }

    fn view(&self) -> McpServerView {
        McpServerView {
            name: self.config.name.clone(),
            enabled: self.config.enabled,
            transport: self.config.transport.clone(),
            command: self.config.command.clone(),
            args: self.config.args.clone(),
            status: self.status.clone(),
            error: self.error.clone(),
            server_info: self.server_info.clone(),
            instructions: self.instructions.clone(),
            tool_count: self.tools.len(),
            resource_count: self.resources.len(),
            updated_at: self.updated_at,
            stderr_tail: self
                .handle
                .as_ref()
                .and_then(|handle| non_empty_tail(&handle.stderr.lock().unwrap(), 2000)),
        }
    }
}

#[derive(Default)]
pub struct McpManager {
    servers: Mutex<HashMap<String, McpServerRuntime>>,
}

impl McpManager {
    pub fn panel_state(&self) -> McpPanelState {
        let servers = self.servers.lock().unwrap();
        let mut server_views = Vec::new();
        let mut tools = Vec::new();
        let mut resources = HashMap::new();
        for server in servers.values() {
            server_views.push(server.view());
            for tool in &server.tools {
                tools.push(tool_view(tool));
            }
            if !server.resources.is_empty() {
                resources.insert(
                    server.config.name.clone(),
                    server.resources.iter().map(resource_view).collect(),
                );
            }
        }
        server_views.sort_by(|a, b| a.name.cmp(&b.name));
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        McpPanelState {
            servers: server_views,
            tools,
            resources,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let servers = self.servers.lock().unwrap();
        servers
            .values()
            .filter(|server| server.status == McpServerStatus::Connected)
            .flat_map(|server| server.tools.iter().map(tool_definition))
            .collect()
    }

    fn resolve_tool(&self, exposed_name: &str) -> Option<(McpClientHandle, String, String)> {
        let servers = self.servers.lock().unwrap();
        for server in servers.values() {
            if server.status != McpServerStatus::Connected {
                continue;
            }
            let Some(handle) = &server.handle else {
                continue;
            };
            if let Some(tool) = server
                .tools
                .iter()
                .find(|tool| tool.exposed_name == exposed_name)
            {
                return Some((
                    handle.clone(),
                    server.config.name.clone(),
                    tool.original_name.clone(),
                ));
            }
        }
        None
    }
}

pub async fn ensure_initialized(state: &crate::AppState) {
    let configs = state.settings.lock().unwrap().mcp_servers.clone();
    let desired = configs
        .iter()
        .map(|config| (config.name.clone(), config.signature()))
        .collect::<HashMap<_, _>>();

    let stale_names = {
        let servers = state.mcp.servers.lock().unwrap();
        servers
            .iter()
            .filter_map(|(name, runtime)| {
                if desired
                    .get(name)
                    .map(|sig| sig != &runtime.signature)
                    .unwrap_or(true)
                {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    for name in stale_names {
        disconnect_server(state, &name).await;
    }

    for config in configs {
        if config.name.trim().is_empty() {
            continue;
        }
        let should_connect = {
            let servers = state.mcp.servers.lock().unwrap();
            !servers.contains_key(&config.name)
        };
        if !should_connect {
            continue;
        }
        if !config.enabled {
            state
                .mcp
                .servers
                .lock()
                .unwrap()
                .insert(config.name.clone(), McpServerRuntime::disabled(config));
            continue;
        }
        match connect_stdio_server(state, config.clone()).await {
            Ok(runtime) => {
                state
                    .mcp
                    .servers
                    .lock()
                    .unwrap()
                    .insert(config.name.clone(), runtime);
            }
            Err(error) => {
                state
                    .mcp
                    .servers
                    .lock()
                    .unwrap()
                    .insert(config.name.clone(), McpServerRuntime::failed(config, error));
            }
        }
    }
}

pub async fn refresh_all(state: &crate::AppState) {
    let names = state
        .mcp
        .servers
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for name in names {
        disconnect_server(state, &name).await;
    }
    ensure_initialized(state).await;
}

pub async fn disconnect_server(state: &crate::AppState, name: &str) {
    let runtime = state.mcp.servers.lock().unwrap().remove(name);
    let Some(runtime) = runtime else {
        return;
    };
    if let Some(handle) = runtime.handle {
        let mut child = handle.child.lock().await;
        let _ = child.kill().await;
        for (_, tx) in handle.pending.lock().unwrap().drain() {
            let _ = tx.send(Err("MCP server disconnected.".to_string()));
        }
    }
}

pub fn panel_state(state: &crate::AppState) -> McpPanelState {
    state.mcp.panel_state()
}

pub fn tool_definitions(state: &crate::AppState) -> Vec<ToolDefinition> {
    state.mcp.tool_definitions()
}

pub fn is_mcp_tool_name(name: &str) -> bool {
    name.starts_with("mcp__") && name.split("__").count() >= 3
}

pub fn permission_summary(state: &crate::AppState, name: &str) -> Option<String> {
    let servers = state.mcp.servers.lock().unwrap();
    for server in servers.values() {
        let Some(tool) = server.tools.iter().find(|tool| tool.exposed_name == name) else {
            continue;
        };
        let risk = risk_for_annotations(&tool.annotations);
        return Some(format!(
            "将调用 MCP server `{}` 的工具 `{}`。风险：{:?}；描述：{}",
            tool.server_name,
            tool.original_name,
            risk,
            cap_chars(tool.description.trim(), 400)
        ));
    }
    None
}

pub async fn call_tool(
    state: &crate::AppState,
    exposed_name: &str,
    args: Value,
) -> Result<String, String> {
    let (handle, server_name, original_tool) = state
        .mcp
        .resolve_tool(exposed_name)
        .ok_or_else(|| format!("MCP 工具 `{exposed_name}` 未连接或不存在。"))?;
    let result = send_request(
        &handle,
        "tools/call",
        json!({
            "name": original_tool,
            "arguments": args,
        }),
        TOOL_TIMEOUT,
    )
    .await
    .map_err(|e| format!("MCP server `{server_name}` 工具调用失败：{e}"))?;
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format_mcp_tool_result(&result));
    }
    Ok(format_mcp_tool_result(&result))
}

async fn connect_stdio_server(
    state: &crate::AppState,
    config: McpServerConfig,
) -> Result<McpServerRuntime, String> {
    if config.command.trim().is_empty() {
        return Err("MCP stdio command 不能为空。".to_string());
    }
    if config.transport != McpTransportKind::Stdio {
        return Err("当前 MCP 第一阶段只支持 stdio transport。".to_string());
    }
    if cfg!(windows) && looks_like_npx(&config.command) {
        return Err(
            "Windows 上请把 npx 包装为 command=`cmd`、args=[\"/c\", \"npx\", ...]。".to_string(),
        );
    }

    let mut command = Command::new(&config.command);
    command
        .args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for env in &config.env {
        if !env.key.trim().is_empty() {
            command.env(env.key.trim(), &env.value);
        }
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动 MCP stdio server 失败：{e}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法打开 MCP server stdin。".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法打开 MCP server stdout。".to_string())?;
    let stderr = child.stderr.take();

    let handle = McpClientHandle {
        writer: Arc::new(tokio::sync::Mutex::new(stdin)),
        child: Arc::new(tokio::sync::Mutex::new(child)),
        pending: Arc::new(Mutex::new(HashMap::new())),
        next_id: Arc::new(AtomicU64::new(1)),
        stderr: Arc::new(Mutex::new(String::new())),
    };
    spawn_stdout_reader(
        handle.clone(),
        stdout,
        state.sandbox_dir.lock().unwrap().clone(),
    );
    if let Some(stderr) = stderr {
        spawn_stderr_reader(handle.stderr.clone(), stderr);
    }

    let initialize = send_request(
        &handle,
        "initialize",
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "roots": {}
            },
            "clientInfo": {
                "name": "demiurge",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        CONNECT_TIMEOUT,
    )
    .await?;
    send_notification(&handle, "notifications/initialized", json!({})).await?;

    let capabilities = initialize
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let server_info = initialize.get("serverInfo").map(server_info_string);
    let instructions = initialize
        .get("instructions")
        .and_then(Value::as_str)
        .map(|s| cap_chars(s, 2048));
    let tools = if capabilities.get("tools").is_some() {
        discover_tools(&handle, &config.name).await?
    } else {
        Vec::new()
    };
    let resources = if capabilities.get("resources").is_some() {
        discover_resources(&handle).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(McpServerRuntime {
        signature: config.signature(),
        config,
        status: McpServerStatus::Connected,
        error: None,
        server_info,
        instructions,
        capabilities,
        tools,
        resources,
        handle: Some(handle),
        updated_at: store::now_millis(),
    })
}

async fn discover_tools(
    handle: &McpClientHandle,
    server_name: &str,
) -> Result<Vec<McpTool>, String> {
    let result = send_request(handle, "tools/list", json!({}), REQUEST_TIMEOUT).await?;
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "MCP tools/list 返回缺少 tools 数组。".to_string())?;
    let mut used_names = HashMap::<String, usize>::new();
    let mut out = Vec::new();
    for tool in tools {
        let original_name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if original_name.is_empty() {
            continue;
        }
        let base_name = build_mcp_tool_name(server_name, &original_name);
        let count = used_names.entry(base_name.clone()).or_insert(0);
        let exposed_name = if *count == 0 {
            base_name
        } else {
            let suffix = stable_hash(&original_name);
            cap_tool_name(format!("{base_name}_{suffix}"))
        };
        *count += 1;
        out.push(McpTool {
            exposed_name,
            server_name: server_name.to_string(),
            original_name,
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .map(sanitize_text)
                .unwrap_or_default(),
            input_schema: normalize_input_schema(tool.get("inputSchema").cloned()),
            annotations: parse_annotations(tool.get("annotations")),
        });
    }
    Ok(out)
}

async fn discover_resources(handle: &McpClientHandle) -> Result<Vec<McpResource>, String> {
    let result = send_request(handle, "resources/list", json!({}), REQUEST_TIMEOUT).await?;
    let resources = result
        .get("resources")
        .and_then(Value::as_array)
        .ok_or_else(|| "MCP resources/list 返回缺少 resources 数组。".to_string())?;
    Ok(resources
        .iter()
        .filter_map(|resource| {
            Some(McpResource {
                uri: resource.get("uri")?.as_str()?.to_string(),
                name: resource
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                description: resource
                    .get("description")
                    .and_then(Value::as_str)
                    .map(sanitize_text),
                mime_type: resource
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect())
}

pub async fn read_resource(
    state: &crate::AppState,
    server_name: &str,
    uri: &str,
) -> Result<String, String> {
    let handle = {
        let servers = state.mcp.servers.lock().unwrap();
        let server = servers
            .get(server_name)
            .ok_or_else(|| format!("MCP server `{server_name}` 不存在。"))?;
        if server.status != McpServerStatus::Connected {
            return Err(format!("MCP server `{server_name}` 未连接。"));
        }
        server
            .handle
            .clone()
            .ok_or_else(|| format!("MCP server `{server_name}` 缺少连接句柄。"))?
    };
    let result = send_request(
        &handle,
        "resources/read",
        json!({ "uri": uri }),
        REQUEST_TIMEOUT,
    )
    .await?;
    Ok(format_mcp_resource_result(&result))
}

fn spawn_stdout_reader(
    handle: McpClientHandle,
    stdout: tokio::process::ChildStdout,
    sandbox_dir: std::path::PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let Ok(message) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    handle_incoming_message(&handle, message, &sandbox_dir).await;
                }
                Ok(None) => break,
                Err(error) => {
                    reject_all_pending(&handle, format!("MCP stdout read failed: {error}"));
                    break;
                }
            }
        }
        reject_all_pending(&handle, "MCP server stdout closed.".to_string());
    });
}

fn spawn_stderr_reader(stderr: Arc<Mutex<String>>, stream: tokio::process::ChildStderr) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut out = stderr.lock().unwrap();
            out.push_str(&line);
            out.push('\n');
            if out.len() > STDERR_CAP {
                let keep_from = out.len().saturating_sub(STDERR_CAP);
                let next = out[keep_from..].to_string();
                *out = next;
            }
        }
    });
}

async fn handle_incoming_message(handle: &McpClientHandle, message: Value, sandbox_dir: &Path) {
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        if message.get("method").is_none() {
            let sender = handle.pending.lock().unwrap().remove(&id);
            if let Some(sender) = sender {
                let payload = if let Some(error) = message.get("error") {
                    Err(format_jsonrpc_error(error))
                } else {
                    Ok(message.get("result").cloned().unwrap_or_else(|| json!({})))
                };
                let _ = sender.send(payload);
            }
            return;
        }
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    if let Some(id) = message.get("id").cloned() {
        let result = match method {
            "roots/list" => Ok(json!({
                "roots": [{
                    "uri": file_uri(sandbox_dir),
                    "name": "Demiurge sandbox"
                }]
            })),
            "ping" => Ok(json!({})),
            _ => Err(json!({
                "code": -32601,
                "message": format!("Demiurge MCP client does not support server request `{method}`.")
            })),
        };
        let response = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        let _ = write_message(handle, response).await;
    }
}

async fn send_request(
    handle: &McpClientHandle,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let id = handle.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    handle.pending.lock().unwrap().insert(id, tx);
    let message = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    if let Err(error) = write_message(handle, message).await {
        handle.pending.lock().unwrap().remove(&id);
        return Err(error);
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(format!("MCP request `{method}` channel closed.")),
        Err(_) => {
            handle.pending.lock().unwrap().remove(&id);
            Err(format!(
                "MCP request `{method}` timed out after {}s.",
                timeout.as_secs()
            ))
        }
    }
}

async fn send_notification(
    handle: &McpClientHandle,
    method: &str,
    params: Value,
) -> Result<(), String> {
    write_message(
        handle,
        json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }),
    )
    .await
}

async fn write_message(handle: &McpClientHandle, message: Value) -> Result<(), String> {
    let mut writer = handle.writer.lock().await;
    let mut line = serde_json::to_string(&message).map_err(|e| e.to_string())?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("写入 MCP stdin 失败：{e}"))?;
    writer
        .flush()
        .await
        .map_err(|e| format!("刷新 MCP stdin 失败：{e}"))
}

fn reject_all_pending(handle: &McpClientHandle, error: String) {
    for (_, tx) in handle.pending.lock().unwrap().drain() {
        let _ = tx.send(Err(error.clone()));
    }
}

fn tool_definition(tool: &McpTool) -> ToolDefinition {
    let risk = risk_for_annotations(&tool.annotations);
    let permission = PermissionPolicy {
        effect: PermissionEffect::Ask,
        scope: PermissionScope::Once,
        reason: "MCP 工具由外部 server 提供，执行前需要确认。",
    };
    ToolDefinition {
        name: Box::leak(tool.exposed_name.clone().into_boxed_str()),
        description: Box::leak(
            format!(
                "MCP tool from server `{}`: {}",
                tool.server_name,
                if tool.description.trim().is_empty() {
                    tool.original_name.as_str()
                } else {
                    tool.description.trim()
                }
            )
            .into_boxed_str(),
        ),
        risk,
        concurrency: if tool.annotations.read_only {
            ToolConcurrency::ParallelSafe
        } else {
            ToolConcurrency::SerialOnly
        },
        permission,
        output_policy: ToolOutputPolicy::TruncateForUi,
        parameters: tool.input_schema.clone(),
    }
}

fn tool_view(tool: &McpTool) -> McpToolView {
    McpToolView {
        name: tool.exposed_name.clone(),
        server_name: tool.server_name.clone(),
        original_name: tool.original_name.clone(),
        title: tool.annotations.title.clone(),
        description: tool.description.clone(),
        risk: risk_for_annotations(&tool.annotations),
        read_only: tool.annotations.read_only,
        destructive: tool.annotations.destructive,
        open_world: tool.annotations.open_world,
    }
}

fn resource_view(resource: &McpResource) -> McpResourceView {
    McpResourceView {
        uri: resource.uri.clone(),
        name: resource.name.clone(),
        description: resource.description.clone(),
        mime_type: resource.mime_type.clone(),
    }
}

fn risk_for_annotations(annotations: &McpAnnotations) -> ToolRisk {
    if annotations.destructive {
        ToolRisk::Mutating
    } else if annotations.open_world {
        ToolRisk::External
    } else if annotations.read_only {
        ToolRisk::ReadOnly
    } else {
        ToolRisk::Privileged
    }
}

fn parse_annotations(value: Option<&Value>) -> McpAnnotations {
    let get_bool = |key: &str| {
        value
            .and_then(|v| v.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    McpAnnotations {
        title: value
            .and_then(|v| v.get("title"))
            .and_then(Value::as_str)
            .map(sanitize_text),
        read_only: get_bool("readOnlyHint"),
        destructive: get_bool("destructiveHint"),
        open_world: get_bool("openWorldHint"),
    }
}

fn normalize_input_schema(schema: Option<Value>) -> Value {
    let Some(mut schema) = schema else {
        return json!({ "type": "object", "properties": {} });
    };
    if !schema.is_object() {
        return json!({ "type": "object", "properties": {} });
    }
    if schema.get("type").is_none() {
        schema["type"] = Value::String("object".to_string());
    }
    if schema.get("properties").is_none() {
        schema["properties"] = json!({});
    }
    schema
}

fn build_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    let server = normalize_segment(server_name, 24);
    let tool = normalize_segment(tool_name, 30);
    cap_tool_name(format!("mcp__{server}__{tool}"))
}

fn cap_tool_name(name: String) -> String {
    if name.len() <= 64 {
        return name;
    }
    let hash = stable_hash(&name);
    let head: String = name.chars().take(55).collect();
    format!("{head}_{hash}")
}

fn normalize_segment(value: &str, max_chars: usize) -> String {
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

fn stable_hash(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:08x}", (hasher.finish() & 0xffff_ffff) as u32)
}

fn sanitize_text(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            *ch == '\n' || *ch == '\r' || *ch == '\t' || !(*ch as u32 <= 0x1f || *ch as u32 == 0x7f)
        })
        .collect::<String>()
}

fn cap_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let head: String = value.chars().take(max_chars).collect();
        format!("{head}… [truncated]")
    }
}

fn non_empty_tail(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= max_chars {
        Some(trimmed.to_string())
    } else {
        Some(
            trimmed
                .chars()
                .rev()
                .take(max_chars)
                .collect::<String>()
                .chars()
                .rev()
                .collect(),
        )
    }
}

fn format_jsonrpc_error(error: &Value) -> String {
    let code = error.get("code").and_then(Value::as_i64);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Unknown MCP error");
    match code {
        Some(code) => format!("JSON-RPC error {code}: {message}"),
        None => message.to_string(),
    }
}

fn format_mcp_resource_result(result: &Value) -> String {
    let Some(contents) = result.get("contents").and_then(Value::as_array) else {
        return format_mcp_tool_result(result);
    };
    let mut out = String::new();
    for item in contents {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        let uri = item.get("uri").and_then(Value::as_str).unwrap_or("");
        let mime = item.get("mimeType").and_then(Value::as_str).unwrap_or("");
        if !uri.is_empty() || !mime.is_empty() {
            out.push_str("Resource");
            if !uri.is_empty() {
                out.push_str(": ");
                out.push_str(uri);
            }
            if !mime.is_empty() {
                out.push_str(" (");
                out.push_str(mime);
                out.push(')');
            }
            out.push_str("\n\n");
        }
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            out.push_str(text);
        } else if let Some(blob) = item.get("blob").and_then(Value::as_str) {
            out.push_str(&format!("[base64 blob omitted; {} chars]", blob.len()));
        } else {
            out.push_str(&serde_json::to_string_pretty(item).unwrap_or_else(|_| item.to_string()));
        }
    }
    if out.trim().is_empty() {
        out = serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string());
    }
    cap_chars(&out, RESULT_CAP_CHARS)
}

fn format_mcp_tool_result(result: &Value) -> String {
    let mut out = String::new();
    if let Some(content) = result.get("content") {
        if let Some(items) = content.as_array() {
            for item in items {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        if !out.is_empty() {
                            out.push_str("\n\n");
                        }
                        out.push_str(text);
                        continue;
                    }
                }
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(
                    &serde_json::to_string_pretty(item).unwrap_or_else(|_| item.to_string()),
                );
            }
        } else {
            out.push_str(
                &serde_json::to_string_pretty(content).unwrap_or_else(|_| content.to_string()),
            );
        }
    }
    if let Some(structured) = result.get("structuredContent") {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("Structured content:\n");
        out.push_str(
            &serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string()),
        );
    }
    if out.trim().is_empty() {
        out = serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string());
    }
    cap_chars(&out, RESULT_CAP_CHARS)
}

fn server_info_string(value: &Value) -> String {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("server");
    let version = value.get("version").and_then(Value::as_str).unwrap_or("");
    if version.is_empty() {
        name.to_string()
    } else {
        format!("{name} {version}")
    }
}

fn file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    if raw.starts_with('/') {
        format!("file://{raw}")
    } else {
        format!("file:///{raw}")
    }
}

fn looks_like_npx(command: &str) -> bool {
    let normalized = command.replace('\\', "/").to_ascii_lowercase();
    normalized == "npx" || normalized.ends_with("/npx") || normalized.ends_with("/npx.cmd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_mcp_tool_name() {
        let name = build_mcp_tool_name("my server", "query.table");
        assert_eq!(name, "mcp__my_server__query_table");
    }

    #[test]
    fn caps_long_tool_name_with_hash() {
        let name = build_mcp_tool_name(
            "server-with-a-very-long-name-that-should-be-capped",
            "tool-with-a-very-long-name-that-should-also-be-capped",
        );
        assert!(name.len() <= 64);
        assert!(name.starts_with("mcp__"));
    }

    #[test]
    fn maps_annotations_to_risk() {
        assert_eq!(
            risk_for_annotations(&McpAnnotations {
                title: None,
                read_only: true,
                destructive: false,
                open_world: false,
            }),
            ToolRisk::ReadOnly
        );
        assert_eq!(
            risk_for_annotations(&McpAnnotations {
                title: None,
                read_only: false,
                destructive: true,
                open_world: false,
            }),
            ToolRisk::Mutating
        );
    }

    #[test]
    fn formats_resource_text_contents() {
        let out = format_mcp_resource_result(&json!({
            "contents": [{
                "uri": "file:///demo.txt",
                "mimeType": "text/plain",
                "text": "hello"
            }]
        }));
        assert!(out.contains("file:///demo.txt"));
        assert!(out.contains("hello"));
        assert!(!out.contains("\"contents\""));
    }
}
