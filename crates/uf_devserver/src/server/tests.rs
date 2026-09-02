use super::*;

use compact_str::CompactString;

fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    std::fs::write(root.join("index.html"), "<!doctype html>\n").unwrap();
    std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();
    (dir, root)
}

/// `DevConfig` and `DevFsConfig` are `#[non_exhaustive]`, so tests adjust a
/// default rather than spelling out every field.
fn config(adjust: impl FnOnce(&mut DevConfig)) -> DevConfig {
    let mut config = DevConfig::default();
    config.port = 0;
    adjust(&mut config);
    config
}

fn ephemeral() -> DevConfig {
    config(|_| {})
}

#[test]
fn binds_loopback_by_default() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    assert!(server.address().ip().is_loopback());
    assert_eq!(server.network_policy().exposure(), Exposure::Loopback);
}

#[test]
fn falls_back_to_an_ephemeral_port_unless_strict() {
    let (_guard, root) = project();
    let first = DevServer::bind(&root, &ephemeral()).unwrap();
    let taken = config(|config| {
        config.port = first.address().port();
        config.strict_port = false;
    });
    let second = DevServer::bind(&root, &taken).unwrap();
    assert_ne!(second.address().port(), 0);
}

#[test]
fn a_strict_port_that_is_taken_is_a_bind_error() {
    let (_guard, root) = project();
    let first = DevServer::bind(&root, &ephemeral()).unwrap();
    let taken = config(|config| {
        config.port = first.address().port();
        config.strict_port = true;
    });
    assert!(matches!(
        DevServer::bind(&root, &taken).unwrap_err(),
        DevServerError::Bind { .. }
    ));
}

/// Whether this machine will let a test bind a routable address at all. Some
/// sandboxes will not, and that is an environment limit, not a regression.
fn can_bind_wildcard() -> bool {
    std::net::TcpListener::bind(("0.0.0.0", 0)).is_ok()
}

#[test]
fn exposure_is_read_from_the_bound_socket() {
    // A routable bind with no allowed hosts must fail to start, whatever the
    // configuration file claims about the posture.
    if !can_bind_wildcard() {
        return;
    }
    let (_guard, root) = project();
    let exposed = config(|config| config.host = CompactString::const_new("0.0.0.0"));
    assert!(matches!(
        DevServer::bind(&root, &exposed).unwrap_err(),
        DevServerError::Network(NetworkPolicyError::ExposedWithoutAllowedHosts)
    ));
}

#[test]
fn an_exposed_bind_with_an_allowlist_starts() {
    if !can_bind_wildcard() {
        return;
    }
    let (_guard, root) = project();
    let exposed = config(|config| {
        config.host = CompactString::const_new("0.0.0.0");
        config.allowed_hosts = vec![CompactString::const_new("dev.internal")];
    });
    let server = DevServer::bind(&root, &exposed).unwrap();
    assert_eq!(server.network_policy().exposure(), Exposure::Exposed);
}

#[test]
fn a_bad_allow_root_is_a_startup_error() {
    let (_guard, root) = project();
    let config = config(|config| config.fs.allow = vec![CompactString::const_new("nowhere")]);
    assert!(matches!(
        DevServer::bind(&root, &config).unwrap_err(),
        DevServerError::Policy(_)
    ));
}

#[test]
fn configured_deny_patterns_are_added_to_the_built_ins() {
    let (_guard, root) = project();
    let config = config(|config| config.fs.deny = vec![CompactString::const_new("*.secret")]);
    let server = DevServer::bind(&root, &config).unwrap();
    let patterns: Vec<&str> = server.fs_policy().deny_patterns().collect();
    assert!(patterns.contains(&".env*"));
    assert!(patterns.contains(&"*.secret"));
}

#[test]
fn the_state_file_describes_the_access_control_posture() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let path = server.write_state(&root).unwrap();
    assert_eq!(path, root.join(STATE_DIR).join(STATE_FILE));

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written["engine"], "uf-native");
    assert_eq!(written["health"], crate::http::HEALTH_TARGET);
    assert_eq!(written["exposure"], "loopback");
    assert_eq!(written["pluginContract"], PLUGIN_CONTRACT);
    assert!(written["port"].as_u64().unwrap() > 0);
    assert_eq!(written["allowedHosts"], serde_json::json!([]));
    assert_eq!(written["allowedOrigins"], serde_json::json!([]));
    assert_eq!(written["fsAllow"], serde_json::json!([root.as_str()]));
    assert!(
        written["fsDeny"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(".env*"))
    );
}

#[test]
fn the_state_file_round_trips() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let path = server.write_state(&root).unwrap();
    let parsed: DevServerState =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed, server.state());
}

#[test]
fn finds_the_end_of_a_request_head() {
    assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\n"), Some(14));
    assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    assert_eq!(find_head_end(b""), None);
}

/// Drive a real socket end to end. `serve_next` blocks, so the request is
/// written from a worker thread while the test thread serves it.
fn round_trip(server: &DevServer, request: String) -> (Status, String) {
    use std::io::{Read, Write};

    let address = server.address();
    let client = std::thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        // Writes are best-effort: an over-long head makes the server stop
        // reading and close, which the client sees as a reset mid-write. That
        // is the server behaving correctly, not a failure to reproduce.
        let _ = stream.write_all(request.as_bytes());
        let _ = stream.flush();
        // Half-close so a request with no head terminator ends promptly rather
        // than waiting out the connection timeout.
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response
    });
    let status = server.serve_next().unwrap();
    (status, client.join().unwrap())
}

#[test]
fn serves_a_file_over_a_real_socket() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let (status, response) = round_trip(
        &server,
        format!(
            "GET /index.html HTTP/1.1\r\nhost: {}\r\n\r\n",
            server.address()
        ),
    );
    assert_eq!(status, Status::Ok);
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with("<!doctype html>\n"));
}

#[test]
fn refuses_a_denied_file_over_a_real_socket() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let (status, response) = round_trip(
        &server,
        format!("GET /.env HTTP/1.1\r\nhost: {}\r\n\r\n", server.address()),
    );
    assert_eq!(status, Status::Forbidden);
    assert!(response.starts_with("HTTP/1.1 403 Forbidden\r\n"));
    assert!(!response.contains("SECRET"));
}

#[test]
fn refuses_a_request_with_no_head_terminator() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let (status, response) = round_trip(&server, "GET /index.html HTTP/1.1\r\n".to_string());
    assert_eq!(status, Status::BadRequest);
    assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
}

#[test]
fn refuses_a_head_larger_than_the_ceiling_over_a_real_socket() {
    let (_guard, root) = project();
    let server = DevServer::bind(&root, &ephemeral()).unwrap();
    let padding = "a".repeat(MAX_REQUEST_HEAD_BYTES);
    let (status, _) = round_trip(
        &server,
        format!(
            "GET /index.html HTTP/1.1\r\nhost: {}\r\nx-pad: {padding}\r\n\r\n",
            server.address()
        ),
    );
    assert_eq!(status, Status::BadRequest);
}
