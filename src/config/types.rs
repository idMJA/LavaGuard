use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YoutubeConfig {
    pub base_url: Option<String>,
    pub override_player_id: Option<String>,
    pub override_player_variant: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub youtube: YoutubeConfig,
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
