/// Configuration from environment variables set by the Zed extension wrapper.
#[derive(Debug, Clone)]
pub struct Config {
    pub neon_api_key: Option<String>,
    pub neon_project_id: Option<String>,
    pub neon_database: Option<String>,
    pub neon_branch: Option<String>,
    pub database_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        fn e(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }
        Self {
            neon_api_key: e("NEON_API_KEY"),
            neon_project_id: None,
            neon_database: None,
            neon_branch: None,
            database_url: e("DATABASE_URL"),
        }
    }

    pub fn api_host(&self) -> &str {
        "https://console.neon.tech/api/v2"
    }

    pub fn has_auth(&self) -> bool {
        self.neon_api_key.is_some()
    }
}
