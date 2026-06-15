use crate::config::Config;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

/// A client for the Neon Management API and Neon SQL-over-HTTP endpoint.
pub struct NeonClient {
    http: Client,
    api_base: String,
    api_key: Option<String>,
}

/// Represents a parsed Postgres connection URI.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

/// Represents a Neon project summary from the API.
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Represents a Neon branch.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub r#default: Option<bool>,
    pub protected: Option<bool>,
}

/// Represents a database in a Neon project.
#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub id: u64,
    pub name: String,
    pub owner_name: Option<String>,
    pub created_at: Option<String>,
}

// --- Neon API response wrappers ---

#[derive(Debug, Deserialize)]
struct ListProjectsResponse {
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct ListBranchesResponse {
    branches: Vec<Branch>,
}

#[derive(Debug, Deserialize)]
struct ListDatabasesResponse {
    databases: Vec<Database>,
}

#[derive(Debug, Deserialize)]
struct ConnectionUriResponse {
    uri: String,
}

// --- SQL query types ---

/// A single SQL query to send to the Neon HTTP SQL endpoint.
#[derive(Debug, Serialize)]
pub(crate) struct SqlQuery {
    pub(crate) query: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) params: Vec<serde_json::Value>,
}

/// Response from the Neon SQL HTTP endpoint.
///
/// Rows are returned as JSON objects (`{"col": val, ...}`) keyed by column name.
#[derive(Debug, Deserialize)]
pub struct SqlResponse {
    pub command: Option<String>,
    pub fields: Option<Vec<Field>>,
    pub rows: Vec<serde_json::Value>,
    #[serde(rename = "rowCount")]
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "dataTypeID")]
    pub data_type_id: Option<u32>,
}

// --- NeonClient impl ---

impl NeonClient {
    pub fn new(config: &Config) -> Self {
        Self {
            http: Client::new(),
            api_base: config.api_host().to_string(),
            api_key: config.neon_api_key.clone(),
        }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    // ------------------------------------------------------------------
    // Neon Management API methods
    // ------------------------------------------------------------------

    pub async fn list_projects(&self) -> Result<Vec<Project>, String> {
        let url = format!("{}/projects", self.api_base);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(self.api_key.as_deref().unwrap_or(""))
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Neon API error {status}: {body}"));
        }

        let data: ListProjectsResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse projects: {e}"))?;

        Ok(data.projects)
    }

    pub async fn list_branches(&self, project_id: &str) -> Result<Vec<Branch>, String> {
        let url = format!("{}/projects/{project_id}/branches", self.api_base);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(self.api_key.as_deref().unwrap_or(""))
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Neon API error {status}: {body}"));
        }

        let data: ListBranchesResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse branches: {e}"))?;

        Ok(data.branches)
    }

    pub async fn list_databases(
        &self,
        project_id: &str,
        branch_id: &str,
    ) -> Result<Vec<Database>, String> {
        let url = format!(
            "{}/projects/{project_id}/branches/{branch_id}/databases",
            self.api_base
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(self.api_key.as_deref().unwrap_or(""))
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Neon API error {status}: {body}"));
        }

        let data: ListDatabasesResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse databases: {e}"))?;

        Ok(data.databases)
    }

    pub async fn get_connection_uri(
        &self,
        project_id: &str,
        branch_id: &str,
        database_name: &str,
        role_name: &str,
    ) -> Result<String, String> {
        let url = format!("{}/projects/{project_id}/connection_uri", self.api_base);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(self.api_key.as_deref().unwrap_or(""))
            .query(&[
                ("branch_id", branch_id),
                ("database_name", database_name),
                ("role_name", role_name),
                ("endpoint_type", "read_write"),
            ])
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Neon API error {status}: {body}"));
        }

        let data: ConnectionUriResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse connection URI: {e}"))?;

        Ok(data.uri)
    }

    pub async fn resolve_database(
        &self,
        project_id: &str,
        branch_id: &str,
        database: Option<&str>,
    ) -> Result<(String, String), String> {
        let dbs = self.list_databases(project_id, branch_id).await?;
        if dbs.is_empty() {
            return Err("No databases found in this branch".to_string());
        }

        if let Some(name) = database {
            let db = dbs.iter().find(|d| d.name == name);
            match db {
                Some(d) => Ok((d.name.clone(), d.owner_name.clone().unwrap_or_default())),
                None => Err(format!("Database '{name}' not found in branch {branch_id}")),
            }
        } else {
            let default = dbs
                .iter()
                .find(|d| d.name == "neondb")
                .or_else(|| dbs.first());
            match default {
                Some(d) => Ok((d.name.clone(), d.owner_name.clone().unwrap_or_default())),
                None => Err("No databases found".to_string()),
            }
        }
    }

    pub async fn resolve_branch(
        &self,
        project_id: &str,
        branch: Option<&str>,
    ) -> Result<String, String> {
        let branches = self.list_branches(project_id).await?;
        if branches.is_empty() {
            return Err("No branches found in this project".to_string());
        }

        if let Some(id) = branch {
            let found = branches.iter().find(|b| b.id == id || b.name == id);
            match found {
                Some(b) => Ok(b.id.clone()),
                None => Err(format!("Branch '{id}' not found in project {project_id}")),
            }
        } else {
            let default = branches.iter().find(|b| b.r#default == Some(true));
            match default {
                Some(b) => Ok(b.id.clone()),
                None => Ok(branches[0].id.clone()),
            }
        }
    }

    // ------------------------------------------------------------------
    // SQL over HTTP
    // ------------------------------------------------------------------

    pub fn parse_connection_uri(uri: &str) -> Result<ConnectionInfo, String> {
        let url = Url::parse(uri).map_err(|e| format!("Invalid connection URI: {e}"))?;

        Ok(ConnectionInfo {
            host: url
                .host_str()
                .ok_or_else(|| "Missing host in connection URI".to_string())?
                .to_string(),
            port: url.port().unwrap_or(5432),
            user: url.username().to_string(),
            password: url.password().unwrap_or("").to_string(),
            dbname: url
                .path_segments()
                .and_then(|mut s| s.next())
                .unwrap_or("neondb")
                .to_string(),
        })
    }

    /// Execute a single SQL query via Neon's HTTP SQL endpoint.
    ///
    /// Uses the serverless driver protocol: POST to `https://{host}/sql`
    /// with the `Neon-Connection-String` header for auth.
    ///
    /// Pooler-host suffixes (`-pooler`) are stripped because the HTTP
    /// SQL endpoint does not route through the connection pooler.
    pub async fn execute_sql(
        &self,
        connection_uri: &str,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<SqlResponse, String> {
        let info = Self::parse_connection_uri(connection_uri)?;
        let host = info.host.replace("-pooler.", ".");
        let sql_url = format!("https://{host}/sql");

        eprintln!("SQL HTTP: POST {sql_url}");

        let query = SqlQuery {
            query: sql.to_string(),
            params: params.to_vec(),
        };

        let resp = self
            .http
            .post(&sql_url)
            .header("Neon-Connection-String", connection_uri)
            .json(&query)
            .send()
            .await
            .map_err(|e| format!("SQL HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("SQL error {status}: {body}"));
        }

        let body_text = resp.text().await.unwrap_or_default();
        eprintln!(
            "SQL raw (first 300): {}",
            &body_text[..body_text.len().min(300)]
        );

        let sql_resp: SqlResponse = serde_json::from_str(&body_text).map_err(|e| {
            format!(
                "Failed to parse SQL response: {e} (body: {})",
                &body_text[..body_text.len().min(200)]
            )
        })?;

        Ok(sql_resp)
    }
}

// --- SqlResponse helpers ---

impl SqlResponse {
    /// Extract rows as `Vec<Vec<String>>` for table display.
    ///
    /// Handles the object-per-row format that Neon returns
    /// (`[{"col": val, ...}, ...]`), preserving column order from `fields`.
    pub fn rows_as_strings(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|row| match row {
                serde_json::Value::Array(arr) => arr.iter().map(|v| value_to_string(v)).collect(),
                serde_json::Value::Object(obj) => {
                    if let Some(fields) = &self.fields {
                        fields
                            .iter()
                            .map(|f| {
                                obj.get(&f.name)
                                    .map(value_to_string)
                                    .unwrap_or_else(|| "NULL".to_string())
                            })
                            .collect()
                    } else {
                        obj.values().map(value_to_string).collect()
                    }
                }
                _ => vec![value_to_string(row)],
            })
            .collect()
    }

    pub fn column_names(&self) -> Vec<String> {
        self.fields
            .as_ref()
            .map(|f| f.iter().map(|fld| fld.name.clone()).collect())
            .unwrap_or_default()
    }

    pub fn row_count(&self) -> u64 {
        self.row_count.unwrap_or(self.rows.len() as u64)
    }
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
