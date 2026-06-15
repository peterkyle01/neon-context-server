use schemars::JsonSchema;
use serde::Deserialize;
use zed::settings::ContextServerSettings;
use zed_extension_api::{
    self as zed, serde_json, Command, ContextServerConfiguration, ContextServerId, Project, Result,
};

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(non_snake_case)]
struct NeonContextServerSettings {
    #[serde(default, alias = "neon_api_key")]
    NEON_API_KEY: String,
    #[serde(default, alias = "neon_project_id")]
    NEON_PROJECT_ID: String,
    #[serde(default = "default_database", alias = "neon_database")]
    NEON_DATABASE: String,
    #[serde(default, alias = "neon_branch")]
    NEON_BRANCH: String,
    #[serde(default, alias = "database_url")]
    DATABASE_URL: String,
    #[serde(default, alias = "neon_api_host")]
    NEON_API_HOST: String,
}

fn default_database() -> String {
    "neondb".to_string()
}

struct NeonContextServerExtension;

impl zed::Extension for NeonContextServerExtension {
    fn new() -> Self {
        Self
    }

    fn context_server_command(
        &mut self,
        _context_server_id: &ContextServerId,
        project: &Project,
    ) -> Result<Command> {
        let settings = ContextServerSettings::for_project("neon-context-server", project)?;
        let Some(settings) = settings.settings else {
            return Err("missing `NEON_API_KEY` setting".into());
        };
        let settings: NeonContextServerSettings =
            serde_json::from_value(settings).map_err(|e| e.to_string())?;

        let binary = concat!(env!("CARGO_MANIFEST_DIR"), "/neon-context-server").to_string();

        Ok(Command {
            command: binary,
            args: vec![],
            env: vec![
                ("NEON_API_KEY".into(), settings.NEON_API_KEY),
                ("NEON_PROJECT_ID".into(), settings.NEON_PROJECT_ID),
                ("NEON_DATABASE".into(), settings.NEON_DATABASE),
                ("NEON_BRANCH".into(), settings.NEON_BRANCH),
                ("DATABASE_URL".into(), settings.DATABASE_URL),
                ("NEON_API_HOST".into(), settings.NEON_API_HOST),
            ],
        })
    }

    fn context_server_configuration(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Option<ContextServerConfiguration>> {
        let installation_instructions =
            include_str!("../configuration/installation_instructions.md").to_string();
        let default_settings = include_str!("../configuration/default_settings.json").to_string();
        let settings_schema =
            serde_json::to_string(&schemars::schema_for!(NeonContextServerSettings))
                .map_err(|e| e.to_string())?;

        Ok(Some(ContextServerConfiguration {
            installation_instructions,
            default_settings,
            settings_schema,
        }))
    }
}

zed::register_extension!(NeonContextServerExtension);
