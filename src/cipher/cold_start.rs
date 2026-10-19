use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColdStartInfo {
    pub client_state: u8,
    pub content_binding: String,
    pub date: String,
    pub keys: [u8; 2],
    pub timestamp: u64,
    pub unknown_val: u8,
}

/// Decodes a base64url string into bytes without external dependencies.
fn decode_base64url(s: &str) -> anyhow::Result<Vec<u8>> {
    let clean = s.trim_end_matches('=');
    let mut bits = 0u32;
    let mut bit_count = 0;
    let mut out = Vec::with_capacity((clean.len() * 3) / 4);

    for &b in clean.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a' + 26),
            b'0'..=b'9' => u32::from(b - b'0' + 52),
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => anyhow::bail!("invalid base64 character"),
        };

        bits = (bits << 6) | val;
        bit_count += 6;

        if bit_count >= 8 {
            bit_count -= 8;
            let byte: u8 = (bits >> bit_count).to_le_bytes()[0];
            out.push(byte);
            bits &= (1 << bit_count) - 1;
        }
    }

    Ok(out)
}

/// Decodes a cold start token into structured metadata.
///
/// # Errors
///
/// Returns an error if the base64 decoding fails, the packet structure is malformed,
/// or the payload length is invalid.
pub fn decode_cold_start_token(token: &str) -> anyhow::Result<ColdStartInfo> {
    let mut packet = decode_base64url(token)?;

    if packet.len() < 10 || packet[0] != 34 {
        anyhow::bail!("invalid cold start token packet");
    }

    let length = packet[1] as usize;
    if packet.len() != length + 2 || length < 8 {
        anyhow::bail!("invalid cold start packet length");
    }

    let payload = &mut packet[2..];
    for i in 2..payload.len() {
        let k = payload[i % 2];
        payload[i] ^= k;
    }

    let b4 = u64::from(payload[4]);
    let b5 = u64::from(payload[5]);
    let b6 = u64::from(payload[6]);
    let b7 = u64::from(payload[7]);
    let timestamp = (b4 << 24) | (b5 << 16) | (b6 << 8) | b7;

    let content_binding = String::from_utf8_lossy(&payload[8..]).to_string();

    // Standard UTC ISO timestamp formatting from unix epoch
    let date = format_timestamp_iso(timestamp);

    Ok(ColdStartInfo {
        client_state: payload[3],
        content_binding,
        date,
        keys: [payload[0], payload[1]],
        timestamp,
        unknown_val: payload[2],
    })
}

fn format_timestamp_iso(seconds: u64) -> String {
    let days_since_epoch = seconds / 86400;
    let sec_in_day = seconds % 86400;

    let hours = sec_in_day / 3600;
    let minutes = (sec_in_day % 3600) / 60;
    let secs = sec_in_day % 60;

    // Simple civil day calculation from day count
    let z = days_since_epoch.cast_signed() + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned();
    let yoe = (doe - doe / 1024 + doe / 1460 - doe / 146_096) / 365;
    let y = yoe.cast_signed() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{secs:02}.000Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_cold_start() {
        // Valid token tested previously: "IhN4TnhPEp0SHEsCEgUAdzElCHYP"
        let token = "IhN4TnhPEp0SHEsCEgUAdzElCHYP";
        let res = decode_cold_start_token(token);
        assert!(res.is_ok());
        let info = res.unwrap();
        assert_eq!(info.content_binding, "3LjKx9Ikp8w");
        assert_eq!(info.client_state, 1);
        assert_eq!(info.keys, [120, 78]);
    }
}
