/// Configuration read from environment variables.
///
/// Environment variables are set by the Zed extension wrapper (src/lib.rs)
/// which reads them from the user's Zed settings.
#[derive(Debug, Clone)]
pub struct Config {
    pub neon_api_key: Option<String>,
    pub neon_project_id: Option<String>,
    pub neon_database: Option<String>,
    pub neon_branch: Option<String>,
    pub database_url: Option<String>,
    pub neon_api_host: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        fn env_optional(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }

        Self {
            neon_api_key: env_optional("NEON_API_KEY"),
            neon_project_id: env_optional("NEON_PROJECT_ID"),
            neon_database: env_optional("NEON_DATABASE"),
            neon_branch: env_optional("NEON_BRANCH"),
            database_url: env_optional("DATABASE_URL"),
            neon_api_host: None,
        }
    }

    pub fn api_host(&self) -> &str {
        self.neon_api_host
            .as_deref()
            .unwrap_or("https://console.neon.tech/api/v2")
    }

    pub fn has_auth(&self) -> bool {
        self.neon_api_key.is_some() || self.database_url.is_some()
    }
}
