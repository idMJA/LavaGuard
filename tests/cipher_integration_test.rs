use lavaguard::cipher::cold_start::decode_cold_start_token;

#[test]
fn test_decode_cold_start_valid() {
    let token = "IhN4TnhPEp0SHEsCEgUAdzElCHYP";
    let decoded = decode_cold_start_token(token).expect("decode valid token");

    assert_eq!(decoded.content_binding, "3LjKx9Ikp8w");
    assert_eq!(decoded.client_state, 1);
    assert_eq!(decoded.keys, [120, 78]);
    assert!(decoded.date.contains("2026-10-17"));
}

#[test]
fn test_decode_cold_start_invalid() {
    let token = "invalid_base64_packet!";
    let err = decode_cold_start_token(token);
    assert!(err.is_err());
}

#[test]
fn test_sts_extraction() {
    let script = r#"
        var desc = "test player";
        var a = { signatureTimestamp: 20745 };
    "#;
    let sts = lavaguard::cipher::player::parse_sts_timestamp(script).expect("parse sts");
    assert_eq!(sts, "20745");
}

#[test]
fn test_url_resolver_assembly() {
    use lavaguard::cipher::types::SolveResult;

    let raw = "https://rr1---sn-4g5ednld.googlevideo.com/videoplayback?expire=12345&n=orig&sig=old";
    let solve_res = SolveResult {
        n: Some("new_n".to_string()),
        sig: Some("new_sig".to_string()),
        resolved_url: None,
        mode: "ejs".to_string(),
    };
    let resolved = lavaguard::cipher::url_solver::assemble_stream_url(raw, None, &solve_res)
        .expect("assemble url");
    assert!(resolved.contains("n=new_n"));
    assert!(resolved.contains("sig=new_sig"));
}
