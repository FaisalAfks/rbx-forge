//! forge's MCP server (`forge mcp`, over stdio): every tool's MCP tools for agents. Each one runs the same `forge`
//! command a person would, in the game's repo, and returns what it printed, so the MCP and the command line can't drift
//! apart, and a command's own printing never mixes into the MCP's stdio. The tools come from each forge tool's
//! `Tool::mcp_tools` (decision 0019); this crate knows none of them by name.
//!
//! A push from here is proposed like any other: it applies only when Apply is clicked in Studio, unless the person in
//! Studio set forge's "Apply pushes" setting not to ask (decisions 0015, 0018). No tool can change that setting.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Result, bail};
use forge_core::tool::{Kind, McpTool};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, transport::stdio};
use serde_json::{Value as Json, json};

/// What every MCP tool takes besides its own arguments.
const REPO: &str = "repo";
const REPO_DESCRIPTION: &str = "The game repo's folder (the one with forge.toml, or below it). Leave out to use the \
                                folder the MCP server was started in.";

const INSTRUCTIONS: &str = "forge syncs a Roblox game between Studio and its git repo. Run tools in the game's repo \
                            (the folder with forge.toml): status first, then pull, diff, push, check. A push applies \
                            only after Apply is clicked in Studio unless the person there chose otherwise; never \
                            assume it applied, read the result.";

/// Runs forge commands in a game's repo.
pub struct Forge {
    /// The forge program to run (this one).
    exe: PathBuf,
    tools: Vec<McpTool>,
}

impl Forge {
    /// The server for these tools. Their names have to be unique, since an MCP client sees one flat list.
    pub fn new(exe: PathBuf, tools: Vec<McpTool>) -> Result<Forge> {
        let mut seen = BTreeSet::new();
        for t in &tools {
            if !seen.insert(t.name) {
                bail!("two forge tools have an MCP tool called {}", t.name);
            }
        }
        Ok(Forge { exe, tools })
    }

    /// The tools as MCP lists them, each with its arguments' JSON Schema.
    pub fn list(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|t| Tool::new(t.name, t.description, Arc::new(schema(t))))
            .collect()
    }

    /// Runs tool `name` with `args`: its command lines one after another, their output joined into one text.
    pub async fn call(&self, name: &str, args: &JsonObject) -> Result<String, String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| format!("forge has no tool {name}"))?;
        check_args(tool, args)?;
        let repo = args.get(REPO).and_then(Json::as_str).map(str::to_string);
        let mut out = Vec::new();
        for line in (tool.commands)(args) {
            out.push(self.run(repo.as_deref(), &line).await?);
        }
        Ok(out.join("\n"))
    }

    /// Runs `forge <args>` in the repo and returns what it printed. A failing command is an error with its output.
    async fn run(&self, repo: Option<&str>, args: &[String]) -> Result<String, String> {
        let mut command = tokio::process::Command::new(&self.exe);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = repo {
            command.current_dir(dir);
        }
        let out = command
            .output()
            .await
            .map_err(|e| format!("can't run forge: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() {
            Ok(text)
        } else {
            Err(text)
        }
    }
}

/// A tool's arguments as JSON Schema: `repo`, then its own.
fn schema(tool: &McpTool) -> JsonObject {
    let mut properties = serde_json::Map::new();
    properties.insert(
        REPO.into(),
        json!({ "description": REPO_DESCRIPTION, "type": ["string", "null"] }),
    );
    let mut required = Vec::new();
    for p in &tool.params {
        let base = match p.kind {
            Kind::Text => "string",
            Kind::Flag => "boolean",
            Kind::Count => "integer",
        };
        let mut property = json!({ "description": p.description });
        if p.required {
            property["type"] = json!(base);
            required.push(p.name);
        } else {
            property["type"] = json!([base, "null"]);
        }
        if p.kind == Kind::Count {
            property["minimum"] = json!(0);
        }
        properties.insert(p.name.into(), property);
    }
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    match schema {
        Json::Object(map) => map,
        _ => unreachable!("built as an object"),
    }
}

/// Every required argument is there, and every argument has its kind, before a command line is built from them.
fn check_args(tool: &McpTool, args: &JsonObject) -> Result<(), String> {
    for p in &tool.params {
        let ok = match (args.get(p.name), p.kind) {
            (None | Some(Json::Null), _) => !p.required,
            (Some(Json::String(_)), Kind::Text) | (Some(Json::Bool(_)), Kind::Flag) => true,
            (Some(Json::Number(n)), Kind::Count) => n.is_u64(),
            _ => false,
        };
        if !ok {
            return Err(format!(
                "{}: argument {} is missing or wrong",
                tool.name, p.name
            ));
        }
    }
    Ok(())
}

impl ServerHandler for Forge {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.server_info = Implementation::new("forge", env!("CARGO_PKG_VERSION"));
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(INSTRUCTIONS.into());
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.unwrap_or_default();
        let result = match self.call(&request.name, &args).await {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]),
        };
        Ok(result.into())
    }
}

/// Serves the tools over stdio until the client goes away. `exe` is the forge program the tools run.
pub async fn serve(exe: PathBuf, tools: Vec<McpTool>) -> Result<()> {
    let service = Forge::new(exe, tools)?.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
