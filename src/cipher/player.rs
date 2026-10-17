use std::sync::OnceLock;

use crate::config::Config;

static PLAYER_ID_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static PLAYER_VARIANT_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static STS_REGEX: OnceLock<regex::Regex> = OnceLock::new();

pub fn normalize_player_url(raw: &str, config: &Config) -> String {
    let trimmed = raw.trim();
    let base = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with('/') {
        format!("https://www.youtube.com{trimmed}")
    } else {
        format!("https://www.youtube.com/{trimmed}")
    };

    let mut result = base;

    if let Some(ref override_id) = config.youtube.override_player_id {
        if !override_id.is_empty() {
            let re = PLAYER_ID_REGEX
                .get_or_init(|| regex::Regex::new(r"/player/[a-zA-Z0-9_-]+/").unwrap());
            result = re
                .replace(&result, format!("/player/{override_id}/"))
                .into_owned();
        }
    }

    if let Some(ref override_variant) = config.youtube.override_player_variant {
        if !override_variant.is_empty() {
            let variant_str = if override_variant.ends_with(".vflset") {
                override_variant.clone()
            } else {
                format!("player_{}.vflset", override_variant.to_lowercase())
            };
            let re = PLAYER_VARIANT_REGEX
                .get_or_init(|| regex::Regex::new(r"/([a-zA-Z0-9_-]+\.vflset)/").unwrap());
            result = re.replace(&result, format!("/{variant_str}/")).into_owned();
        }
    }

    result
}

pub fn parse_sts_timestamp(player_script: &str) -> anyhow::Result<String> {
    let re =
        STS_REGEX.get_or_init(|| regex::Regex::new(r"(?:signatureTimestamp|sts):(\d+)").unwrap());
    re.captures(player_script)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| anyhow::anyhow!("could not find sts timestamp in player script"))
}
