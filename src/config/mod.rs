use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub server: ServerConfig,
    pub youtube: YoutubeConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct YoutubeConfig {
    pub base_url: Option<String>,
    pub override_player_id: Option<String>,
    pub override_player_variant: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3000,
                auth_key: None,
            },
            youtube: YoutubeConfig {
                base_url: Some("https://www.youtube.com".to_string()),
                override_player_id: None,
                override_player_variant: None,
            },
        }
    }
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Config {
    let path_ref = path.as_ref();
    if !path_ref.exists() {
        return Config::default();
    }

    match fs::read_to_string(path_ref) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("failed parsing config.toml ({e}), falling back to default");
            Config::default()
        }),
        Err(e) => {
            tracing::warn!("failed reading config.toml ({e}), falling back to default");
            Config::default()
        }
    }
}
