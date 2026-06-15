mod commands;
mod config;
mod neon_client;

use commands::ContextItem;
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

#[derive(Debug, Deserialize)]
struct RpcMessage {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}
#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: String,
    id: Value,
    result: Value,
}

fn tool_definitions() -> Value {
    json!([
        {"name":"neon_query","description":"Execute a read-only SQL query against your Neon database","inputSchema":{"type":"object","properties":{"sql":{"type":"string","description":"The SQL query (SELECT only)"},"project_id":{"type":"string","description":"Neon project ID. Auto-discovered if omitted"}},"required":["sql"]}},
        {"name":"neon_list_tables","description":"List all user tables in the database","inputSchema":{"type":"object","properties":{"project_id":{"type":"string","description":"Neon project ID. Auto-discovered if omitted"}}}},
        {"name":"neon_describe_table","description":"Show columns, indexes, and constraints for a table. Supports schema.table syntax.","inputSchema":{"type":"object","properties":{"table":{"type":"string","description":"Table name (e.g. 'users' or 'public.users')"},"project_id":{"type":"string","description":"Neon project ID"}},"required":["table"]}},
        {"name":"neon_schema","description":"Display the full database schema. Optionally filter by schema name.","inputSchema":{"type":"object","properties":{"schema":{"type":"string","description":"Optional: filter to a specific schema"},"project_id":{"type":"string","description":"Neon project ID"}}}},
        {"name":"neon_list_schemas","description":"List all schemas in the database","inputSchema":{"type":"object","properties":{"project_id":{"type":"string","description":"Neon project ID"}}}},
        {"name":"neon_list_databases","description":"List databases in the project","inputSchema":{"type":"object","properties":{"project_id":{"type":"string","description":"Neon project ID"}}}},
        {"name":"neon_list_projects","description":"List all Neon projects. Use this to discover available project IDs for other tools.","inputSchema":{"type":"object","properties":{}}},
        {"name":"neon_list_branches","description":"List all branches in a project","inputSchema":{"type":"object","properties":{"project_id":{"type":"string","description":"Neon project ID"}}}},
        {"name":"neon_explain","description":"Show the PostgreSQL execution plan for a query (human-readable)","inputSchema":{"type":"object","properties":{"sql":{"type":"string","description":"The SQL query to explain"},"project_id":{"type":"string","description":"Neon project ID"}},"required":["sql"]}},
        {"name":"neon_get_connection_string","description":"Get the PostgreSQL connection URI for configuring your application","inputSchema":{"type":"object","properties":{"project_id":{"type":"string","description":"Neon project ID"}}}}
    ])
}

#[tokio::main]
async fn main() {
    let config = Config::from_env();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: RpcMessage = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let id = match msg.id {
            Some(id) => id,
            None => continue,
        };

        let result = match msg.method.as_str() {
            "initialize" => {
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"neon-context-server","version":"0.1.0"}})
            }
            "tools/list" => json!({"tools": tool_definitions()}),
            "tools/call" => {
                let params = msg.params.unwrap_or_default();
                let name = params["name"].as_str().unwrap_or("");
                let args = params["arguments"].clone();
                let pid = args["project_id"].as_str().map(|s| s.to_string());
                let mut c = config.clone();
                if let Some(p) = pid {
                    c.neon_project_id = Some(p);
                }

                let (argv, cmd) = match name {
                    "neon_query" => (
                        vec![args["sql"].as_str().unwrap_or("").to_string()],
                        "/neon:query",
                    ),
                    "neon_list_tables" => (vec![], "/neon:list-tables"),
                    "neon_describe_table" => (
                        vec![args["table"].as_str().unwrap_or("").to_string()],
                        "/neon:describe-table",
                    ),
                    "neon_schema" => {
                        let schema = args["schema"]
                            .as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        (
                            if schema.is_empty() {
                                vec![]
                            } else {
                                vec![schema]
                            },
                            "/neon:schema",
                        )
                    }
                    "neon_list_schemas" => (vec![], "/neon:list-schemas"),
                    "neon_list_databases" => (vec![], "/neon:list-databases"),
                    "neon_list_projects" => (vec![], "/neon:list-projects"),
                    "neon_list_branches" => (vec![], "/neon:list-branches"),
                    "neon_explain" => (
                        vec![args["sql"].as_str().unwrap_or("").to_string()],
                        "/neon:explain",
                    ),
                    "neon_get_connection_string" => (vec![], "/neon:get-connection-string"),
                    _ => (vec![], ""),
                };

                match commands::handle_command(&c, cmd, &argv).await {
                    Ok(items) => json!({"content":[{"type":"text","text":format_items(&items)}]}),
                    Err(e) => {
                        json!({"content":[{"type":"text","text":format!("Error: {e}")}],"isError":true})
                    }
                }
            }
            "notifications/initialized" => continue,
            _ => json!({"error":{"code":-32601,"message":format!("Unknown: {}",msg.method)}}),
        };

        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result,
        };
        writeln!(out, "{}", serde_json::to_string(&resp).unwrap_or_default()).ok();
        out.flush().ok();
    }
}

fn format_items(items: &[ContextItem]) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            ContextItem::Title { text } => {
                out.push_str(&format!("# {text}\n\n"));
            }
            ContextItem::Header { text } => {
                out.push_str(&format!("## {text}\n\n"));
            }
            ContextItem::Text { text } => {
                out.push_str(&format!("{text}\n"));
            }
            ContextItem::Table { header, rows, .. } => {
                let mut widths: Vec<usize> = header.iter().map(|h| h.len()).collect();
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i < widths.len() {
                            widths[i] = widths[i].max(cell.len());
                        }
                    }
                }
                for (i, h) in header.iter().enumerate() {
                    out.push_str(&format!("| {:<w$} ", h, w = widths[i]));
                }
                out.push_str("|\n");
                for (i, _) in header.iter().enumerate() {
                    out.push_str(&format!("|{}", "-".repeat(widths[i] + 2)));
                }
                out.push_str("|\n");
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i < widths.len() {
                            out.push_str(&format!("| {:<w$} ", cell, w = widths[i]));
                        }
                    }
                    out.push_str("|\n");
                }
                out.push('\n');
            }
        }
    }
    out
}
