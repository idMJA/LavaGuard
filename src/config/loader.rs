use std::fs;
use std::path::Path;

use super::types::Config;

pub fn read_config_file<P: AsRef<Path>>(path: P) -> Config {
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
