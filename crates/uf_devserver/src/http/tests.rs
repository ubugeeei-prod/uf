use super::*;

use camino::Utf8PathBuf;

use crate::network::Exposure;
use crate::target::TargetError;

fn head(raw: &str) -> Result<RequestHead<'_>, HttpError> {
    RequestHead::parse(raw.as_bytes())
}

// --- request head parsing ---------------------------------------------------

#[test]
fn parses_a_minimal_request() {
    let request = head("GET /app/main.js HTTP/1.1\r\nhost: localhost:5173\r\n\r\n").unwrap();
    assert_eq!(request.method(), Method::Get);
    assert_eq!(request.target(), "/app/main.js");
    assert_eq!(request.host(), Some("localhost:5173"));
    assert_eq!(request.origin(), None);
}

#[test]
fn header_names_are_case_insensitive() {
    let request =
        head("GET / HTTP/1.1\r\nHOST: localhost\r\nOrIgIn: http://a.test\r\n\r\n").unwrap();
    assert_eq!(request.host(), Some("localhost"));
    assert_eq!(request.origin(), Some("http://a.test"));
}

#[test]
fn keeps_only_host_and_origin() {
    // The guard is that there is nowhere else for a header to go. This asserts
    // the shape rather than the absence: `RequestHead` has four fields, and two
    // of them are headers.
    let request = head(concat!(
        "GET /a.js HTTP/1.1\r\n",
        "host: localhost\r\n",
        "x-middleware-subrequest: middleware\r\n",
        "x-forwarded-host: evil.test\r\n",
        "x-rewrite-url: /.env\r\n",
        "x-original-url: /.env\r\n",
        "origin: http://localhost\r\n",
        "\r\n"
    ))
    .unwrap();
    assert_eq!(request.target(), "/a.js");
    assert_eq!(request.host(), Some("localhost"));
    assert_eq!(request.origin(), Some("http://localhost"));
}

#[test]
fn rejects_a_duplicate_host_header() {
    assert_eq!(
        head("GET / HTTP/1.1\r\nhost: localhost\r\nhost: evil.test\r\n\r\n").unwrap_err(),
        HttpError::DuplicateHeader { name: "Host" }
    );
}

#[test]
fn rejects_a_duplicate_origin_header() {
    assert_eq!(
        head(
            "OPTIONS / HTTP/1.1\r\nhost: localhost\r\norigin: http://a\r\norigin: http://b\r\n\r\n"
        )
        .unwrap_err(),
        HttpError::DuplicateHeader { name: "Origin" }
    );
}

#[test]
fn rejects_a_malformed_request_line() {
    for raw in [
        "GET\r\n\r\n",
        "GET /\r\n\r\n",
        "GET / HTTP/1.1 extra\r\n\r\n",
        "GET  HTTP/1.1\r\n\r\n",
    ] {
        assert!(
            matches!(
                head(raw).unwrap_err(),
                HttpError::MalformedRequestLine | HttpError::UnsupportedVersion
            ),
            "{raw:?} was accepted"
        );
    }
}

#[test]
fn rejects_an_unsupported_version() {
    assert_eq!(
        head("GET / HTTP/2\r\n\r\n").unwrap_err(),
        HttpError::UnsupportedVersion
    );
}

#[test]
fn rejects_an_unsupported_method() {
    for method in ["POST", "PUT", "DELETE", "PATCH", "TRACE", "CONNECT", "get"] {
        assert_eq!(
            head(&format!("{method} / HTTP/1.1\r\n\r\n")).unwrap_err(),
            HttpError::UnsupportedMethod,
            "{method} was accepted"
        );
    }
}

#[test]
fn rejects_a_head_over_the_byte_ceiling() {
    let raw = format!(
        "GET / HTTP/1.1\r\nhost: localhost\r\nx-pad: {}\r\n\r\n",
        "a".repeat(MAX_REQUEST_HEAD_BYTES)
    );
    assert_eq!(head(&raw).unwrap_err(), HttpError::HeadTooLarge);
}

#[test]
fn rejects_more_header_lines_than_the_ceiling() {
    let mut raw = String::from("GET / HTTP/1.1\r\n");
    for index in 0..=MAX_HEADER_LINES {
        raw.push_str(&format!("x-{index}: v\r\n"));
    }
    raw.push_str("\r\n");
    assert_eq!(head(&raw).unwrap_err(), HttpError::TooManyHeaders);
}

#[test]
fn rejects_a_header_line_without_a_colon() {
    assert_eq!(
        head("GET / HTTP/1.1\r\nnonsense\r\n\r\n").unwrap_err(),
        HttpError::MalformedHeader
    );
}

#[test]
fn rejects_a_head_that_is_not_utf8() {
    assert_eq!(
        RequestHead::parse(b"GET /\xff HTTP/1.1\r\n\r\n").unwrap_err(),
        HttpError::NonUtf8
    );
}

// --- statuses ---------------------------------------------------------------

#[test]
fn status_codes_and_reasons_are_stable() {
    assert_eq!(Status::Ok.code(), 200);
    assert_eq!(Status::BadRequest.code(), 400);
    assert_eq!(Status::Forbidden.code(), 403);
    assert_eq!(Status::NotFound.code(), 404);
    assert_eq!(Status::MethodNotAllowed.code(), 405);
    assert_eq!(Status::PayloadTooLarge.code(), 413);
    assert_eq!(Status::InternalServerError.code(), 500);
    assert_eq!(Status::Forbidden.to_string(), "403 Forbidden");
    assert!(Status::Ok.is_success());
    assert!(!Status::Forbidden.is_success());
}

#[test]
fn a_grammar_failure_is_a_bad_request() {
    assert_eq!(
        status_for(&AccessDenied::InvalidTarget(TargetError::NotOriginForm)),
        Status::BadRequest
    );
    assert_eq!(status_for(&AccessDenied::DoubleEncoded), Status::BadRequest);
    assert_eq!(
        status_for(&AccessDenied::ForbiddenByte { byte: 0 }),
        Status::BadRequest
    );
}

#[test]
fn a_policy_refusal_is_forbidden() {
    assert_eq!(status_for(&AccessDenied::Escape), Status::Forbidden);
    assert_eq!(
        status_for(&AccessDenied::FilesystemPrefix),
        Status::Forbidden
    );
    assert_eq!(
        status_for(&AccessDenied::Denied(PolicyDenial::DeniedByPattern {
            path: Utf8PathBuf::from(".env"),
            pattern: ".env*".into(),
        })),
        Status::Forbidden
    );
}

// --- responses --------------------------------------------------------------

#[test]
fn a_refusal_carries_no_body() {
    let response = Response::refusal(Status::Forbidden);
    assert!(response.body.is_empty());
    assert_eq!(response.loader, "none");
}

#[test]
fn a_response_never_reflects_the_request() {
    let bytes = Response::refusal(Status::Forbidden).to_bytes();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("HTTP/1.1 403 Forbidden\r\n"));
    assert!(text.contains("content-length: 0\r\n"));
    assert!(text.contains("x-content-type-options: nosniff\r\n"));
    assert!(text.contains("cache-control: no-store\r\n"));
    assert!(
        !text
            .to_ascii_lowercase()
            .contains("access-control-allow-origin")
    );
}

#[test]
fn a_body_response_reports_its_own_length() {
    let response = Response {
        status: Status::Ok,
        content_type: "text/plain; charset=utf-8",
        loader: "module",
        body: b"hello".to_vec(),
    };
    let text = String::from_utf8(response.to_bytes()).unwrap();
    assert!(text.contains("content-length: 5\r\n"));
    assert!(text.ends_with("\r\n\r\nhello"));
}

// --- the whole pipeline -----------------------------------------------------

struct Fixture {
    _dir: tempfile::TempDir,
    fs: FsPolicy,
    network: NetworkPolicy,
}

impl Fixture {
    fn new() -> Self {
        Self::with_network(NetworkPolicy::loopback())
    }

    fn with_network(network: NetworkPolicy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        std::fs::write(root.join("index.html"), "<!doctype html>\n").unwrap();
        std::fs::write(root.join("main.js"), "export default 1;\n").unwrap();
        std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        Self {
            _dir: dir,
            fs: FsPolicy::with_defaults(&root).unwrap(),
            network,
        }
    }

    fn request(&self, raw: &str) -> Response {
        let request = RequestHead::parse(raw.as_bytes()).unwrap();
        respond(&request, &self.fs, &self.network)
    }

    fn get(&self, target: &str) -> Response {
        self.request(&format!(
            "GET {target} HTTP/1.1\r\nhost: localhost:5173\r\n\r\n"
        ))
    }
}

#[test]
fn serves_a_project_file() {
    let fixture = Fixture::new();
    let response = fixture.get("/main.js");
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.content_type, "text/javascript; charset=utf-8");
    assert_eq!(response.loader, "module");
    assert_eq!(response.body, b"export default 1;\n");
}

#[test]
fn serves_the_health_endpoint() {
    let fixture = Fixture::new();
    let response = fixture.get(HEALTH_TARGET);
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.content_type, "application/json");
    assert_eq!(response.body, br#"{"status":"ok","engine":"uf-native"}"#);
}

#[test]
fn the_health_endpoint_does_not_match_with_a_query() {
    // A control route that tolerates a query is a control route with a second,
    // less-tested spelling.
    let fixture = Fixture::new();
    assert_eq!(fixture.get("/__uf/health?raw").status, Status::NotFound);
}

#[test]
fn a_head_request_gets_the_headers_without_the_body() {
    let fixture = Fixture::new();
    let response = fixture.request("HEAD /main.js HTTP/1.1\r\nhost: localhost\r\n\r\n");
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.content_type, "text/javascript; charset=utf-8");
    assert!(response.body.is_empty());
}

#[test]
fn a_denied_file_is_forbidden() {
    let fixture = Fixture::new();
    assert_eq!(fixture.get("/.env").status, Status::Forbidden);
    assert!(fixture.get("/.env").body.is_empty());
}

#[test]
fn an_unknown_file_is_not_found() {
    assert_eq!(Fixture::new().get("/missing.js").status, Status::NotFound);
}

#[test]
fn an_invalid_target_is_a_bad_request() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.get("http://evil.test/.env").status,
        Status::BadRequest
    );
    assert_eq!(fixture.get("*").status, Status::BadRequest);
}

#[test]
fn a_rebinding_host_is_refused_before_the_path_is_looked_at() {
    let fixture = Fixture::new();
    let response = fixture.request("GET /main.js HTTP/1.1\r\nhost: evil.test\r\n\r\n");
    assert_eq!(response.status, Status::Forbidden);
    assert!(response.body.is_empty());
}

#[test]
fn a_missing_host_header_is_a_bad_request() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.request("GET /main.js HTTP/1.1\r\n\r\n").status,
        Status::BadRequest
    );
}

#[test]
fn a_preflight_from_an_unlisted_origin_is_forbidden() {
    let fixture = Fixture::new();
    let response = fixture.request(
        "OPTIONS /main.js HTTP/1.1\r\nhost: localhost\r\norigin: http://evil.test\r\n\r\n",
    );
    assert_eq!(response.status, Status::Forbidden);
}

#[test]
fn a_preflight_from_a_listed_origin_still_has_nothing_to_preflight() {
    let network = NetworkPolicy::new(
        Exposure::Loopback,
        Vec::<&str>::new(),
        ["http://localhost:5173"],
    )
    .unwrap();
    let fixture = Fixture::with_network(network);
    let response = fixture.request(
        "OPTIONS /main.js HTTP/1.1\r\nhost: localhost\r\norigin: http://localhost:5173\r\n\r\n",
    );
    assert_eq!(response.status, Status::MethodNotAllowed);
}

#[test]
fn a_cross_origin_simple_get_is_served_without_cors_headers() {
    // The browser, not this server, is what stops the other origin reading it.
    let fixture = Fixture::new();
    let response = fixture
        .request("GET /main.js HTTP/1.1\r\nhost: localhost\r\norigin: http://evil.test\r\n\r\n");
    assert_eq!(response.status, Status::Ok);
    let text = String::from_utf8(response.to_bytes()).unwrap();
    assert!(
        !text
            .to_ascii_lowercase()
            .contains("access-control-allow-origin")
    );
}

#[test]
fn no_inbound_header_changes_what_is_served() {
    // CVE-2025-29927's bug class, asserted end to end: the same target with a
    // pile of dispatch-flavoured headers produces byte-identical responses.
    let fixture = Fixture::new();
    let plain = fixture.request("GET /main.js HTTP/1.1\r\nhost: localhost\r\n\r\n");
    let loaded = fixture.request(concat!(
        "GET /main.js HTTP/1.1\r\n",
        "host: localhost\r\n",
        "x-middleware-subrequest: middleware:middleware:middleware\r\n",
        "x-forwarded-host: evil.test\r\n",
        "x-forwarded-proto: https\r\n",
        "x-original-url: /.env\r\n",
        "x-rewrite-url: /.env\r\n",
        "x-uf-loader: raw\r\n",
        "x-http-method-override: POST\r\n",
        "authorization: Bearer nonsense\r\n",
        "\r\n"
    ));
    assert_eq!(plain, loaded);
}

#[test]
fn a_dispatch_header_cannot_reach_a_denied_file() {
    let fixture = Fixture::new();
    let response = fixture.request(concat!(
        "GET /main.js HTTP/1.1\r\n",
        "host: localhost\r\n",
        "x-original-url: /.env\r\n",
        "x-rewrite-url: /.env\r\n",
        "\r\n"
    ));
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.body, b"export default 1;\n");
}
