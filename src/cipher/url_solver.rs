use url::Url;

use super::types::SolveResult;

pub fn assemble_stream_url(
    stream_url: &str,
    sig_key: Option<&str>,
    solved: &SolveResult,
) -> anyhow::Result<String> {
    let mut parsed_url = Url::parse(stream_url)?;

    let pairs: Vec<(String, String)> = parsed_url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    parsed_url.set_query(None);
    let mut query = parsed_url.query_pairs_mut();
    let sig_param_key = sig_key.unwrap_or("sig");

    for (k, v) in pairs {
        if k == "s" {
            continue;
        }
        if k == "n" {
            if let Some(ref resolved_n) = solved.n {
                query.append_pair("n", resolved_n);
                continue;
            }
        }
        query.append_pair(&k, &v);
    }

    if let Some(ref resolved_sig) = solved.sig {
        query.append_pair(sig_param_key, resolved_sig);
    }

    drop(query);
    Ok(parsed_url.to_string())
}
