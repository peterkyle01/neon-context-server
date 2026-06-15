use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use zed::settings::ContextServerSettings;
use zed_extension_api::{
    self as zed, serde_json, Command, ContextServerConfiguration, ContextServerId, Project, Result,
};

const BINARY: &str = "neon-context-server";

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(non_snake_case)]
struct NeonContextServerSettings {
    #[serde(default, alias = "neon_api_key")]
    NEON_API_KEY: String,
}

struct NeonContextServerExtension {
    cached: Option<String>,
}

impl zed::Extension for NeonContextServerExtension {
    fn new() -> Self {
        Self { cached: None }
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
        let s: NeonContextServerSettings =
            serde_json::from_value(settings).map_err(|e| e.to_string())?;

        let binary = self.find_binary()?;
        Ok(Command {
            command: binary,
            args: vec![],
            env: vec![("NEON_API_KEY".into(), s.NEON_API_KEY)],
        })
    }

    fn context_server_configuration(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Option<ContextServerConfiguration>> {
        Ok(Some(ContextServerConfiguration {
            installation_instructions: include_str!(
                "../configuration/installation_instructions.md"
            )
            .to_string(),
            default_settings: include_str!("../configuration/default_settings.json").to_string(),
            settings_schema: serde_json::to_string(&schemars::schema_for!(
                NeonContextServerSettings
            ))
            .map_err(|e| e.to_string())?,
        }))
    }
}

impl NeonContextServerExtension {
    fn find_binary(&mut self) -> Result<String> {
        if let Some(ref p) = self.cached {
            if fs::metadata(p).map_or(false, |m| m.is_file()) {
                return Ok(p.clone());
            }
        }

        // Production: download from GitHub release
        if let Ok(release) = zed::latest_github_release(
            "peterkyle01/neon-context-server",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            let (platform, arch) = zed::current_platform();
            let asset_name = format!(
                "{BINARY}-{os}-{arch}.tar.gz",
                BINARY = BINARY,
                os = match platform {
                    zed::Os::Mac => "macos",
                    zed::Os::Linux => "linux",
                    _ => "linux",
                },
                arch = match arch {
                    zed::Architecture::Aarch64 => "arm64",
                    _ => "x86_64",
                },
            );

            if let Some(asset) = release.assets.iter().find(|a| a.name == asset_name) {
                let dir = format!("{BINARY}-{}", release.version);
                fs::create_dir_all(&dir).ok();
                let path = format!("{dir}/{BINARY}");

                if !fs::metadata(&path).map_or(false, |m| m.is_file()) {
                    zed::download_file(&asset.download_url, &dir, zed::DownloadedFileType::GzipTar)
                        .map_err(|e| format!("download: {e}"))?;
                    zed::make_file_executable(&path)?;
                    // Clean old versions
                    if let Ok(entries) = fs::read_dir(".") {
                        for e in entries.flatten() {
                            let n = e.file_name();
                            if n.to_str() != Some(&dir)
                                && n.to_str().map_or(false, |n| n.starts_with(BINARY))
                            {
                                fs::remove_dir_all(e.path()).ok();
                            }
                        }
                    }
                }

                self.cached = Some(path.clone());
                return Ok(path);
            }
        }

        // Dev fallback: binary at the extension source root
        let dev = format!("{}/{BINARY}", env!("CARGO_MANIFEST_DIR"));
        self.cached = Some(dev.clone());
        return Ok(dev);
    }
}

zed::register_extension!(NeonContextServerExtension);
