use crate::config::Config;
use crate::neon_client::NeonClient;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug)]
pub enum ContextItem {
    Title {
        text: String,
    },
    Header {
        text: String,
    },
    Text {
        text: String,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

impl ContextItem {
    pub fn title(text: impl Into<String>) -> Self {
        Self::Title { text: text.into() }
    }
    pub fn header(text: impl Into<String>) -> Self {
        Self::Header { text: text.into() }
    }
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
    pub fn table(header: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self::Table { header, rows }
    }
}

pub async fn handle_command(
    config: &Config,
    command: &str,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    if !config.has_auth() {
        return Err("No Neon API key configured. Set NEON_API_KEY in your MCP settings.".into());
    }
    let client = NeonClient::new(config);
    match command {
        "/neon:query" => handle_query(&client, config, args).await,
        "/neon:schema" => handle_schema(&client, config, args).await,
        "/neon:list-tables" => handle_list_tables(&client, config, args).await,
        "/neon:describe-table" => handle_describe_table(&client, config, args).await,
        "/neon:list-databases" => handle_list_databases(&client, config).await,
        "/neon:list-projects" => handle_list_projects(&client).await,
        "/neon:list-branches" => handle_list_branches(&client, config).await,
        "/neon:explain" => handle_explain(&client, config, args).await,
        "/neon:list-schemas" => handle_list_schemas(&client, config).await,
        "/neon:get-connection-string" => handle_get_connection_string(&client, config).await,
        _ => Err(format!("Unknown command: {command}")),
    }
}

// ── Connection resolution ──

async fn resolve_connection(client: &NeonClient, config: &Config) -> Result<String, String> {
    if let Some(ref db_url) = config.database_url {
        return Ok(db_url.clone());
    }
    let project_id = resolve_project_id(client, config).await?;
    let branch_id = client
        .resolve_branch(&project_id, config.neon_branch.as_deref())
        .await?;
    let (db_name, owner) = client
        .resolve_database(&project_id, &branch_id, config.neon_database.as_deref())
        .await?;
    let uri = client
        .get_connection_uri(&project_id, &branch_id, &db_name, &owner)
        .await?;
    Ok(uri)
}

async fn resolve_project_id(client: &NeonClient, config: &Config) -> Result<String, String> {
    if let Some(pid) = &config.neon_project_id {
        return Ok(pid.clone());
    }
    let projects = client.list_projects().await?;
    match projects.len() {
        0 => Err("No Neon projects found. Create one at https://console.neon.tech".to_string()),
        1 => Ok(projects[0].id.clone()),
        n => {
            let mut msg = format!("Found {n} Neon projects. Pass `project_id` to pick one:\n");
            for p in &projects {
                msg.push_str(&format!("  {}  →  {}\n", p.id, p.name));
            }
            Err(msg)
        }
    }
}

// ── P0: Write protection ──

const DESTRUCTIVE_KEYWORDS: &[&str] = &[
    "DROP", "DELETE", "UPDATE", "INSERT", "TRUNCATE", "ALTER", "CREATE", "GRANT", "REVOKE", "COPY",
    "VACUUM", "REINDEX", "CLUSTER", "LOCK", "ANALYZE", "COMMENT",
];

fn is_readonly_sql(sql: &str) -> bool {
    // Strip all comments and string literals, then tokenize
    let clean = strip_sql_literals(&strip_block_comments(&strip_line_comments(sql)));
    let tokens: Vec<String> = clean
        .to_uppercase()
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ';' || c == ',')
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if tokens.is_empty() {
        return true;
    }
    if tokens[0] == "EXPLAIN" {
        return true;
    }

    // Block SELECT INTO (creates tables) unless preceded by INSERT
    for i in 1..tokens.len() {
        if tokens[i] == "INTO" && tokens[i - 1] != "INSERT" {
            return false;
        }
    }

    // Block any destructive keyword token
    for tok in &tokens {
        if *tok == "UPDATE" && tokens.first().map(|s| s.as_str()) == Some("SELECT") {
            continue;
        }
        if DESTRUCTIVE_KEYWORDS.contains(&tok.as_str()) {
            return false;
        }
    }
    true
}

/// Strip SQL string literals: '...', E'...', $$...$$, $tag$...$tag$
fn strip_sql_literals(sql: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // E'...' escaped string
        if i + 2 < chars.len() && (chars[i] == 'E' || chars[i] == 'e') && chars[i + 1] == '\'' {
            i += 3;
            while i < chars.len() {
                if chars[i] == '\'' && (i == 2 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                if chars[i] == '\''
                    && i > 2
                    && chars[i - 1] == '\\'
                    && (i < 3 || chars[i - 2] != '\\')
                {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Regular 'string'
        if chars[i] == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        // $tag$...$tag$ dollar quoting
        if chars[i] == '$' {
            let start = i;
            i += 1;
            // Find end of opening tag
            while i < chars.len() && chars[i] != '$' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            } // skip closing $
            let tag = &chars[start..i].iter().collect::<String>();
            // Find matching closing tag
            while i + tag.len() <= chars.len() {
                let slice: String = chars[i..i + tag.len()].iter().collect();
                if slice == *tag {
                    i += tag.len();
                    break;
                }
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn strip_line_comments(sql: &str) -> String {
    // Remove -- comments (everything from -- to end of line)
    let mut out = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] == '-' {
            // Skip until end of line
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn strip_block_comments(sql: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

// ── P3: Clean error messages ──

fn clean_error(e: &str) -> String {
    // Our HTTP client wraps JSON errors like:
    // "SQL error 400 Bad Request: {"message":"...","file":"...",...}"
    // Extract the JSON portion, parse it, and return only the message.
    if let Some(json_start) = e.find('{') {
        let json_part = &e[json_start..];
        if let Ok(v) = serde_json::from_str::<Value>(json_part) {
            if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("");
                let hint = v.get("hint").and_then(|c| c.as_str()).unwrap_or("");
                let mut out = msg.to_string();
                if !code.is_empty() {
                    out.push_str(&format!(" [{code}]"));
                }
                if !hint.is_empty() {
                    out.push_str(&format!("\nHint: {hint}"));
                }
                return out;
            }
        }
    }
    // Fallback: return the original error as-is
    e.to_string()
}

// ── P2: Row limit ──

const DEFAULT_ROW_LIMIT: usize = 500;

// ── P1: Schema-qualified table name parsing ──

fn parse_table_ref(raw: &str) -> (Option<String>, String) {
    let parts: Vec<&str> = raw.splitn(2, '.').collect();
    if parts.len() == 2 {
        (Some(parts[0].to_string()), parts[1].to_string())
    } else {
        (None, raw.to_string())
    }
}

// ── P7: Format explain plan as human-readable text ──

fn format_explain_plan(json_plan: &str) -> String {
    let v: Value = match serde_json::from_str(json_plan) {
        Ok(v) => v,
        Err(_) => return json_plan.to_string(),
    };
    let plans = v.as_array().map(|a| a.to_vec()).unwrap_or_default();
    let mut out = String::new();
    for (i, plan) in plans.iter().enumerate() {
        if i > 0 {
            out.push_str("---\n");
        }
        let node = &plan["Plan"];
        format_node(node, &mut out, 0);
        if let Some(qid) = plan.get("Query Identifier").and_then(|v| v.as_i64()) {
            out.push_str(&format!("Query ID: {qid}\n"));
        }
    }
    out
}

fn format_node(node: &Value, out: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    let node_type = node["Node Type"].as_str().unwrap_or("?");
    let relation = node
        .get("Relation Name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let alias = node.get("Alias").and_then(|v| v.as_str()).unwrap_or("");
    let rows = node.get("Plan Rows").and_then(|v| v.as_i64()).unwrap_or(0);
    let startup = node
        .get("Startup Cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let total = node
        .get("Total Cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let label = if !relation.is_empty() {
        if alias.is_empty() || alias == relation {
            format!("on {relation}")
        } else {
            format!("on {relation} as {alias}")
        }
    } else {
        String::new()
    };

    out.push_str(&format!(
        "{indent}→ {node_type} {label} (cost={startup}..{total} rows={rows})\n"
    ));

    if let Some(plans) = node.get("Plans").and_then(|v| v.as_array()) {
        for child in plans {
            format_node(child, out, depth + 1);
        }
    }
}

// ── Command handlers ──

async fn handle_query(
    client: &NeonClient,
    config: &Config,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    let sql = args.join(" ").trim().to_string();
    if sql.is_empty() {
        return Err("Usage: provide a SQL query".into());
    }

    // Allow a single trailing semicolon (possibly followed by -- comment or whitespace)
    let clean = strip_sql_literals(&strip_block_comments(&sql));
    let trimmed = clean.trim();
    let sc_positions: Vec<usize> = trimmed.match_indices(';').map(|(i, _)| i).collect();
    let after_last_sc = if let Some(&pos) = sc_positions.last() {
        &trimmed[pos + 1..]
    } else {
        ""
    };
    let only_comment_after =
        after_last_sc.trim().is_empty() || after_last_sc.trim().starts_with("--");
    let ok_trailing = sc_positions.len() == 1 && only_comment_after;
    let in_do_block = trimmed.to_uppercase().starts_with("DO ");
    if !ok_trailing && !in_do_block && !sc_positions.is_empty() {
        return Err("Multi-statement queries are not supported. Run one query at a time.".into());
    }
    if !is_readonly_sql(&sql) {
        return Err(
            "Write operations (INSERT, UPDATE, DELETE, DROP, etc.) are not allowed. \
             Only SELECT and EXPLAIN queries are accepted. \
             Use the Neon Console for schema changes."
                .into(),
        );
    }

    // Block SHOW ALL / SHOW neon.* — leaks internal Neon infrastructure
    let show_upper = sql.trim().to_uppercase();
    if show_upper.starts_with("SHOW ALL") || show_upper.starts_with("SHOW NEON.") {
        return Err("SHOW ALL / SHOW neon.* is blocked (leaks internal info). Use SHOW <non-neon-var> instead.".into());
    }

    // Block queries against pg_settings (same info disclosure as SHOW ALL)
    let upper = sql.to_uppercase();
    if upper.contains("PG_SETTINGS")
        || upper.contains("PG_HBA_FILE_RULES")
        || upper.contains("PG_FILE_SETTINGS")
    {
        return Err(
            "Access to pg_settings and related system catalogs is blocked for security.".into(),
        );
    }

    let conn_uri = resolve_connection(client, config).await?;
    let result = client
        .execute_sql(&conn_uri, &sql, &[])
        .await
        .map_err(|e| clean_error(e.as_str()))?;

    let columns = result.column_names();
    let rows = result.rows_as_strings();

    let mut items = Vec::new();
    if columns.is_empty() {
        items.push(ContextItem::text("Query OK (0 rows returned)"));
    } else {
        let total_rows = rows.len();
        let (display_cols, display_rows) = if total_rows > DEFAULT_ROW_LIMIT {
            let mut r = rows;
            r.truncate(DEFAULT_ROW_LIMIT);
            (columns, r)
        } else {
            (columns, rows)
        };
        items.push(ContextItem::table(display_cols, display_rows));
        let label = if total_rows > DEFAULT_ROW_LIMIT {
            format!("Showing {DEFAULT_ROW_LIMIT} of {total_rows} rows. Add LIMIT to your query for more.")
        } else {
            format!("{total_rows} row{}", if total_rows == 1 { "" } else { "s" })
        };
        items.push(ContextItem::text(label));
    }
    Ok(items)
}

async fn handle_schema(
    client: &NeonClient,
    config: &Config,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    let schema_filter = args.first().cloned();
    let conn_uri = resolve_connection(client, config).await?;

    let where_clause = if let Some(ref s) = schema_filter {
        format!("AND c.table_schema = '{s}'")
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, c.is_nullable, c.column_default, c.ordinal_position \
         FROM information_schema.columns c \
         WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema') {where_clause} \
         ORDER BY c.table_schema, c.table_name, c.ordinal_position"
    );
    let resp = client
        .execute_sql(&conn_uri, &sql, &[])
        .await
        .map_err(|e| clean_error(e.as_str()))?;

    let mut items = vec![ContextItem::title("Database Schema")];
    let col_rows = resp.rows_as_strings();
    let table_set: HashSet<String> = col_rows
        .iter()
        .map(|r| format!("{}.{}", r[0], r[1]))
        .collect();
    items.push(ContextItem::text(format!(
        "{} tables, {} columns total\n",
        table_set.len(),
        resp.row_count()
    )));

    let mut current: Option<String> = None;
    let mut buf: Vec<Vec<String>> = Vec::new();
    for row in &col_rows {
        let full = format!("{}.{}", row[0], row[1]);
        if current.as_deref() != Some(&full) {
            if let Some(ref prev) = current {
                if !buf.is_empty() {
                    items.push(ContextItem::header(prev.clone()));
                    items.push(ContextItem::table(
                        vec![
                            "Column".into(),
                            "Type".into(),
                            "Nullable".into(),
                            "Default".into(),
                        ],
                        buf.clone(),
                    ));
                }
                buf.clear();
            }
            current = Some(full);
        }
        buf.push(vec![
            row[2].clone(),
            row[3].clone(),
            row[4].clone(),
            row[5].clone(),
        ]);
    }
    if let Some(ref prev) = current {
        if !buf.is_empty() {
            items.push(ContextItem::header(prev.clone()));
            items.push(ContextItem::table(
                vec![
                    "Column".into(),
                    "Type".into(),
                    "Nullable".into(),
                    "Default".into(),
                ],
                buf,
            ));
        }
    }
    Ok(items)
}

async fn handle_list_tables(
    client: &NeonClient,
    config: &Config,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    let schema_filter = args.first().cloned();
    let conn_uri = resolve_connection(client, config).await?;
    let where_clause = if let Some(ref s) = schema_filter {
        format!("AND table_schema = '{s}'")
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT table_schema, table_name, table_type FROM information_schema.tables \
         WHERE table_schema NOT IN ('pg_catalog','information_schema') {where_clause} \
         ORDER BY table_schema, table_name"
    );
    let result = client
        .execute_sql(&conn_uri, &sql, &[])
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let mut items = vec![ContextItem::title("Tables")];
    let rows = result.rows_as_strings();
    items.push(ContextItem::text(format!(
        "{} table{}",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    )));
    if rows.is_empty() {
        items.push(ContextItem::text("No user tables found."));
    } else {
        items.push(ContextItem::table(result.column_names(), rows));
    }
    Ok(items)
}

async fn handle_describe_table(
    client: &NeonClient,
    config: &Config,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    let raw = args.first().cloned().unwrap_or_default();
    if raw.is_empty() {
        return Err("Usage: provide a table name".into());
    }
    let (schema, table) = parse_table_ref(&raw);

    let conn_uri = resolve_connection(client, config).await?;

    // P1: Schema-aware column query
    let (col_sql, col_params): (String, Vec<Value>) = if let Some(ref s) = schema {
        (format!(
            "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, pgd.description \
             FROM information_schema.columns c \
             LEFT JOIN pg_catalog.pg_statio_all_tables st ON c.table_schema=st.schemaname AND c.table_name=st.relname \
             LEFT JOIN pg_catalog.pg_description pgd ON pgd.objoid=st.relid AND pgd.objsubid=c.ordinal_position \
             WHERE c.table_schema=$1 AND c.table_name=$2 ORDER BY c.ordinal_position"
        ), vec![Value::String(s.clone()), Value::String(table.clone())])
    } else {
        ("SELECT c.table_schema, c.column_name, c.data_type, c.is_nullable, c.column_default, pgd.description \
          FROM information_schema.columns c \
          LEFT JOIN pg_catalog.pg_statio_all_tables st ON c.table_schema=st.schemaname AND c.table_name=st.relname \
          LEFT JOIN pg_catalog.pg_description pgd ON pgd.objoid=st.relid AND pgd.objsubid=c.ordinal_position \
          WHERE c.table_name=$1 ORDER BY c.ordinal_position".to_string(),
         vec![Value::String(table.clone())])
    };

    let result = client
        .execute_sql(&conn_uri, &col_sql, &col_params)
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let rows = result.rows_as_strings();
    if rows.is_empty() {
        return Err(format!(
            "Table '{raw}' not found in the database. Check the table name and try again."
        ));
    }

    let resolved_schema = if let Some(ref s) = schema {
        s.clone()
    } else {
        rows[0][0].clone()
    };
    let offset = if schema.is_some() { 0 } else { 1 };

    let mut items = vec![ContextItem::title(format!(
        "Table: {resolved_schema}.{table}"
    ))];
    items.push(ContextItem::text(format!(
        "{} column{}",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    )));

    let header = vec![
        "Column".into(),
        "Type".into(),
        "Nullable".into(),
        "Default".into(),
        "Description".into(),
    ];
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r[offset].clone(),
                r[offset + 1].clone(),
                r[offset + 2].clone(),
                r[offset + 3].clone(),
                r.get(offset + 4).cloned().unwrap_or_default(),
            ]
        })
        .collect();
    items.push(ContextItem::table(header, table_rows));

    // Indexes — P6: consistent kb units
    let idx_sql = "SELECT i.relname, pg_get_indexdef(i.oid), pg_size_pretty(pg_relation_size(i.oid)) \
                   FROM pg_class t JOIN pg_index ix ON t.oid=ix.indrelid JOIN pg_class i ON i.oid=ix.indexrelid \
                   WHERE t.relname=$1 AND t.relkind='r'";
    let idx_result = client
        .execute_sql(&conn_uri, idx_sql, &[Value::String(table.clone())])
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let idx_rows = idx_result.rows_as_strings();
    if !idx_rows.is_empty() {
        items.push(ContextItem::header("Indexes"));
        // Normalize sizes to kB
        let normalized: Vec<Vec<String>> = idx_rows
            .iter()
            .map(|r| {
                let size = &r[2];
                let normalized_size = if size.contains("bytes") {
                    format!(
                        "{} kB",
                        size.replace(" bytes", "").parse::<f64>().unwrap_or(0.0) / 1024.0
                    )
                } else {
                    size.clone()
                };
                vec![r[0].clone(), r[1].clone(), normalized_size]
            })
            .collect();
        items.push(ContextItem::table(
            vec!["Name".into(), "Definition".into(), "Size".into()],
            normalized,
        ));
    }

    // Constraints
    let con_sql = "SELECT tc.constraint_name, tc.constraint_type, pg_get_constraintdef(cc.oid) \
                   FROM information_schema.table_constraints tc JOIN pg_catalog.pg_constraint cc ON tc.constraint_name=cc.conname \
                   WHERE tc.table_name=$1";
    let con_result = client
        .execute_sql(&conn_uri, con_sql, &[Value::String(table.clone())])
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let con_rows = con_result.rows_as_strings();
    if !con_rows.is_empty() {
        items.push(ContextItem::header("Constraints"));
        let table: Vec<Vec<String>> = con_rows
            .iter()
            .map(|r| vec![r[0].clone(), r[1].clone(), r[2].clone()])
            .collect();
        items.push(ContextItem::table(
            vec!["Name".into(), "Type".into(), "Definition".into()],
            table,
        ));
    }
    Ok(items)
}

async fn handle_list_databases(
    client: &NeonClient,
    config: &Config,
) -> Result<Vec<ContextItem>, String> {
    if let Some(ref db_url) = config.database_url {
        let sql = "SELECT datname, pg_size_pretty(pg_database_size(datname)) as size FROM pg_database WHERE datistemplate=false ORDER BY datname";
        let result = client
            .execute_sql(db_url, sql, &[])
            .await
            .map_err(|e| clean_error(e.as_str()))?;
        let mut items = vec![ContextItem::title("Databases")];
        items.push(ContextItem::table(
            result.column_names(),
            result.rows_as_strings(),
        ));
        return Ok(items);
    }
    if !client.has_api_key() {
        return Err("Neon API key required".into());
    }
    let project_id = resolve_project_id(client, config).await?;
    let branch_id = client
        .resolve_branch(&project_id, config.neon_branch.as_deref())
        .await?;
    let databases = client.list_databases(&project_id, &branch_id).await?;
    let mut items = vec![ContextItem::title("Neon Databases")];
    items.push(ContextItem::text(format!(
        "Project: {project_id}, Branch: {branch_id}\n"
    )));
    let rows: Vec<Vec<String>> = databases
        .iter()
        .map(|db| {
            vec![
                db.name.clone(),
                db.owner_name.clone().unwrap_or_default(),
                db.created_at.clone().unwrap_or_default(),
            ]
        })
        .collect();
    items.push(ContextItem::table(
        vec!["Name".into(), "Owner".into(), "Created".into()],
        rows,
    ));
    Ok(items)
}

async fn handle_list_projects(client: &NeonClient) -> Result<Vec<ContextItem>, String> {
    if !client.has_api_key() {
        return Err("Neon API key required".into());
    }
    let projects = client.list_projects().await?;
    let mut items = vec![ContextItem::title("Neon Projects")];
    items.push(ContextItem::text(format!(
        "{} project(s). Use `project_id` from this list in other tools:",
        projects.len()
    )));
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|p| {
            vec![
                p.id.clone(),
                p.name.clone(),
                p.created_at.clone().unwrap_or_default(),
            ]
        })
        .collect();
    items.push(ContextItem::table(
        vec!["ID".into(), "Name".into(), "Created".into()],
        rows,
    ));
    Ok(items)
}

async fn handle_list_branches(
    client: &NeonClient,
    config: &Config,
) -> Result<Vec<ContextItem>, String> {
    if !client.has_api_key() {
        return Err("Neon API key required".into());
    }
    let project_id = resolve_project_id(client, config).await?;
    let branches = client.list_branches(&project_id).await?;
    let mut items = vec![ContextItem::title("Neon Branches")];
    items.push(ContextItem::text(format!(
        "Project: {project_id} — {} branch(es)",
        branches.len()
    )));
    let rows: Vec<Vec<String>> = branches
        .iter()
        .map(|b| {
            vec![
                b.id.clone(),
                b.name.clone(),
                b.parent_id.clone().unwrap_or_default(),
                if b.r#default == Some(true) {
                    "Yes".into()
                } else {
                    "No".into()
                },
                if b.protected == Some(true) {
                    "Yes".into()
                } else {
                    "No".into()
                },
            ]
        })
        .collect();
    items.push(ContextItem::table(
        vec![
            "ID".into(),
            "Name".into(),
            "Parent".into(),
            "Default".into(),
            "Protected".into(),
        ],
        rows,
    ));
    Ok(items)
}

async fn handle_explain(
    client: &NeonClient,
    config: &Config,
    args: &[String],
) -> Result<Vec<ContextItem>, String> {
    let sql = args.join(" ").trim().to_string();
    if sql.is_empty() {
        return Err("Usage: provide a SQL query".into());
    }
    let explain_sql = format!("EXPLAIN (ANALYZE false, VERBOSE, FORMAT JSON) {sql}");
    let conn_uri = resolve_connection(client, config).await?;
    let result = client
        .execute_sql(&conn_uri, &explain_sql, &[])
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let mut items = vec![ContextItem::title("Query Execution Plan")];
    let rows = result.rows_as_strings();
    if let Some(first_row) = rows.first() {
        if let Some(plan_json) = first_row.first() {
            let human = format_explain_plan(plan_json);
            items.push(ContextItem::text(human));
        } else {
            items.push(ContextItem::text("(empty plan)"));
        }
    } else {
        items.push(ContextItem::text("(no result)"));
    }
    Ok(items)
}

// ── P4: List schemas ──

async fn handle_list_schemas(
    client: &NeonClient,
    config: &Config,
) -> Result<Vec<ContextItem>, String> {
    let conn_uri = resolve_connection(client, config).await?;
    let sql = "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ('pg_catalog','information_schema') AND schema_name NOT LIKE 'pg_%' ORDER BY schema_name";
    let result = client
        .execute_sql(&conn_uri, sql, &[])
        .await
        .map_err(|e| clean_error(e.as_str()))?;
    let mut items = vec![ContextItem::title("Schemas")];
    let rows = result.rows_as_strings();
    items.push(ContextItem::table(result.column_names(), rows));
    Ok(items)
}

// ── P5: Get connection string ──

async fn handle_get_connection_string(
    client: &NeonClient,
    config: &Config,
) -> Result<Vec<ContextItem>, String> {
    if !client.has_api_key() {
        return Err("Neon API key required for get_connection_string".into());
    }
    let project_id = resolve_project_id(client, config).await?;
    let branch_id = client
        .resolve_branch(&project_id, config.neon_branch.as_deref())
        .await?;
    let (db_name, owner) = client
        .resolve_database(&project_id, &branch_id, config.neon_database.as_deref())
        .await?;
    let uri = client
        .get_connection_uri(&project_id, &branch_id, &db_name, &owner)
        .await?;
    let masked = mask_password(&uri);
    let mut items = vec![ContextItem::title("Connection String")];
    items.push(ContextItem::text(format!(
        "Project: {project_id}\nBranch: {branch_id}\nDatabase: {db_name}\n\n```\n{masked}\n```"
    )));
    Ok(items)
}

fn mask_password(uri: &str) -> String {
    if let Some(at) = uri.find('@') {
        if let Some(colon) = uri[..at].rfind(':') {
            return format!("{}:***{}", &uri[..colon], &uri[at..]);
        }
    }
    uri.to_string()
}
