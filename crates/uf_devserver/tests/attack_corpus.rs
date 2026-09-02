//! The dev server attack corpus.
//!
//! Every row here is a request that must not produce a file. The table is the
//! regression test `docs/security.md` points at for its "Dev server and file
//! serving" rows, and it is meant to grow: when a new bypass is imagined or
//! published, it becomes a row before it becomes a fix.
//!
//! Rows are driven through the whole HTTP surface — request head, network
//! allowlists, target grammar, resolution, response — so a row proves the
//! *server* refuses the request, not merely that some function did.

use camino::{Utf8Path, Utf8PathBuf};
use uf_devserver::hmr::update_target;
use uf_devserver::resolve::{AccessDenied, resolve_with_policy};
use uf_devserver::target::RequestTarget;
use uf_devserver::{FsPolicy, Loader, NetworkPolicy, RequestHead, Response, Status, respond};

/// Marker written into every file the server must never serve. If it appears in
/// a response body, something leaked.
const SECRET: &str = "uf-devserver-secret-marker";

/// A project laid out to give every attack in the corpus something to aim at.
///
/// A denial that happens only because the file is missing proves nothing, so
/// every denied path in the table exists on disk here.
struct Fixture {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
    fs: FsPolicy,
    network: NetworkPolicy,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();

        for (relative, contents) in [
            ("index.html", "<!doctype html>\n"),
            ("app/main.js", "export default 1;\n"),
            ("app/logo.svg", "<svg/>\n"),
            ("a file.js", "spaced\n"),
            ("café.js", "unicode\n"),
            ("nested/deep/mod.js", "deep\n"),
            ("environment.js", "not a secret\n"),
        ] {
            write(&root, relative, contents);
        }
        for relative in [
            ".env",
            ".env.local",
            ".env.",
            ".env ",
            "config/.env.local",
            ".git/config",
            "certs/server.pem",
            "certs/server.key",
            "certs/server.crt",
            ".uf/dev-server.json",
            "..../.env",
        ] {
            write(&root, relative, &format!("{SECRET}\n"));
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", root.join("passwd.js")).unwrap();
            std::os::unix::fs::symlink("/etc", root.join("etc")).unwrap();
            std::os::unix::fs::symlink(".env", root.join("innocent.js")).unwrap();
        }

        Self {
            _dir: dir,
            fs: FsPolicy::with_defaults(&root).unwrap(),
            network: NetworkPolicy::loopback(),
            root,
        }
    }

    fn request(&self, raw: &str) -> Response {
        let head = match RequestHead::parse(raw.as_bytes()) {
            Ok(head) => head,
            // A head this server cannot parse is a `400`, which is exactly what
            // the socket loop sends. Modelling it here keeps malformed-head rows
            // in the same table as everything else.
            Err(_) => return Response::refusal(Status::BadRequest),
        };
        respond(&head, &self.fs, &self.network)
    }

    fn get(&self, target: &str) -> Response {
        self.request(&format!(
            "GET {target} HTTP/1.1\r\nhost: localhost:5173\r\n\r\n"
        ))
    }
}

fn write(root: &Utf8PathBuf, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, contents).unwrap();
}

/// One attack: the request target, what it is trying, and the status the server
/// must answer with.
struct Attack {
    target: &'static str,
    trying: &'static str,
    expect: Status,
}

const BAD_REQUEST: Status = Status::BadRequest;
const FORBIDDEN: Status = Status::Forbidden;

/// The corpus.
///
/// `400` means the request was refused as ungrammatical before any path work;
/// `403` means the access decision refused the resolved path. Both are
/// refusals. The distinction is recorded so that a row silently changing which
/// guard caught it shows up as a test failure rather than passing quietly.
///
/// One row per line on purpose, so the corpus reads as a list of attacks rather
/// than a wall of struct literals. `rustfmt` is told to leave it alone.
#[rustfmt::skip]
const CORPUS: &[Attack] = &[
    // -- CVE-2025-30208: query suffixes that survived the deny check ---------
    Attack { target: "/.env",                trying: "the plain denied file",                 expect: FORBIDDEN },
    Attack { target: "/.env?raw",            trying: "a loader query",                        expect: FORBIDDEN },
    Attack { target: "/.env?raw??",          trying: "the published `?raw??` bypass",         expect: FORBIDDEN },
    Attack { target: "/.env?import&raw??",   trying: "the published `?import&raw??` bypass",  expect: FORBIDDEN },
    Attack { target: "/.env?inline",         trying: "the inline loader",                     expect: FORBIDDEN },
    Attack { target: "/.env?url",            trying: "the url loader",                        expect: FORBIDDEN },
    Attack { target: "/.env?worker",         trying: "the worker loader",                     expect: FORBIDDEN },
    Attack { target: "/.env?t=1&raw??&import", trying: "loaders among cache busters",         expect: FORBIDDEN },
    Attack { target: "/.env?%72aw",          trying: "a percent-encoded loader key",          expect: FORBIDDEN },
    Attack { target: "/.env?raw&inline",     trying: "two loaders at once",                   expect: BAD_REQUEST },
    // -- CVE-2025-31125: traversal wearing a loader query --------------------
    Attack { target: "/app/../.env?import",  trying: "traversal plus the import marker",      expect: FORBIDDEN },
    Attack { target: "/app/../.env?raw",     trying: "traversal plus a loader",               expect: FORBIDDEN },
    Attack { target: "/index.html%2f..%2f.env", trying: "encoded traversal through a file",   expect: FORBIDDEN },
    // -- traversal, in every spelling ----------------------------------------
    Attack { target: "/../.env",             trying: "one level up",                          expect: FORBIDDEN },
    Attack { target: "/../../.env",          trying: "two levels up",                         expect: FORBIDDEN },
    Attack { target: "/../../../../../../etc/passwd", trying: "climbing to the system",       expect: FORBIDDEN },
    Attack { target: "/..%2f..%2f.env",      trying: "encoded separators",                    expect: FORBIDDEN },
    Attack { target: "/%2e%2e/%2e%2e/.env",  trying: "encoded dots",                          expect: FORBIDDEN },
    Attack { target: "/%2E%2E/%2E%2E/.env",  trying: "encoded dots, uppercase hex",           expect: FORBIDDEN },
    Attack { target: "/%2e%2e%2f%2e%2e%2fetc%2fpasswd", trying: "everything encoded",         expect: FORBIDDEN },
    Attack { target: "/..%5c..%5c.env",      trying: "encoded backslash separators",          expect: FORBIDDEN },
    Attack { target: "/....//.env",          trying: "the `....//` normalizer trick",         expect: FORBIDDEN },
    Attack { target: "/....\\\\.env",        trying: "`....\\\\` on any platform",            expect: FORBIDDEN },
    Attack { target: "/....//.env?raw",      trying: "the `....//` trick plus a loader",      expect: FORBIDDEN },
    Attack { target: "/%252e%252e/.env",     trying: "double encoding",                       expect: BAD_REQUEST },
    Attack { target: "/%25252e%25252e/.env", trying: "triple encoding",                       expect: BAD_REQUEST },
    Attack { target: "/%c0%ae%c0%ae/.env",   trying: "overlong UTF-8 dots",                   expect: BAD_REQUEST },
    // -- CVE-2025-62522: the backslash separator ------------------------------
    Attack { target: "/.env\\",              trying: "a literal trailing backslash",          expect: FORBIDDEN },
    Attack { target: "/.env%5C",             trying: "an encoded trailing backslash",         expect: FORBIDDEN },
    Attack { target: "/.env/",               trying: "a trailing slash",                      expect: FORBIDDEN },
    Attack { target: "/.env%20",             trying: "a trailing space",                      expect: FORBIDDEN },
    Attack { target: "/.env.",               trying: "a trailing dot",                        expect: FORBIDDEN },
    Attack { target: "/config\\.env.local",  trying: "a backslash as a separator",            expect: FORBIDDEN },
    // -- CVE-2025-32395: request targets that are not origin-form -------------
    Attack { target: "http://evil.test/.env",  trying: "absolute-form",                       expect: BAD_REQUEST },
    Attack { target: "https://evil.test/.env", trying: "absolute-form over TLS",              expect: BAD_REQUEST },
    Attack { target: "//evil.test/.env",     trying: "a network-path reference",              expect: BAD_REQUEST },
    Attack { target: "evil.test:443",        trying: "authority-form",                        expect: BAD_REQUEST },
    Attack { target: "*",                    trying: "asterisk-form",                         expect: BAD_REQUEST },
    Attack { target: ".env",                 trying: "a relative target",                     expect: BAD_REQUEST },
    Attack { target: "/.env#.js",            trying: "a fragment",                            expect: BAD_REQUEST },
    // -- byte-level smuggling -------------------------------------------------
    Attack { target: "/.env%00.js",          trying: "a poisoned NUL byte",                   expect: BAD_REQUEST },
    Attack { target: "/.env%0d%0aX-Injected:%201", trying: "CRLF injection",                  expect: BAD_REQUEST },
    Attack { target: "/.env%09",             trying: "an encoded tab",                        expect: BAD_REQUEST },
    Attack { target: "/%ff%fe.env",          trying: "a non-UTF-8 path",                      expect: BAD_REQUEST },
    Attack { target: "/%ed%a0%80.env",       trying: "a lone surrogate",                      expect: BAD_REQUEST },
    Attack { target: "/.env%",               trying: "a truncated escape",                    expect: BAD_REQUEST },
    Attack { target: "/.env%2",              trying: "a half-written escape",                 expect: BAD_REQUEST },
    Attack { target: "/.env%zz",             trying: "a non-hex escape",                      expect: BAD_REQUEST },
    // -- the `/@fs/` escape hatch this server does not have -------------------
    Attack { target: "/@fs/etc/passwd",      trying: "Vite's absolute-path prefix",           expect: FORBIDDEN },
    Attack { target: "/@fs//etc/passwd",     trying: "the prefix with a doubled separator",   expect: FORBIDDEN },
    Attack { target: "/@fs%2Fetc%2Fpasswd",  trying: "the prefix, encoded",                   expect: FORBIDDEN },
    Attack { target: "/./@fs/etc/passwd",    trying: "the prefix behind a dot segment",       expect: FORBIDDEN },
    Attack { target: "/app/../@fs/etc/passwd", trying: "the prefix behind a traversal",       expect: FORBIDDEN },
    Attack { target: "/@fs/C:/Windows/win.ini", trying: "the prefix with a drive letter",     expect: FORBIDDEN },
    // -- encoded spellings of a denied name -----------------------------------
    Attack { target: "/.%65nv",              trying: "an encoded letter",                     expect: FORBIDDEN },
    Attack { target: "/%2eenv",              trying: "an encoded leading dot",                expect: FORBIDDEN },
    Attack { target: "/.env.local",          trying: "a dotenv variant",                      expect: FORBIDDEN },
    Attack { target: "/config/.env.local",   trying: "a dotenv in a subdirectory",            expect: FORBIDDEN },
    Attack { target: "/nested/../config/.env.local", trying: "a nested dotenv via traversal", expect: FORBIDDEN },
    // -- other denied classes -------------------------------------------------
    Attack { target: "/.git/config",         trying: "VCS metadata",                          expect: FORBIDDEN },
    Attack { target: "/.git/../.git/config", trying: "VCS metadata via traversal",            expect: FORBIDDEN },
    Attack { target: "/certs/server.pem",    trying: "a certificate",                         expect: FORBIDDEN },
    Attack { target: "/certs/server.key",    trying: "a private key",                         expect: FORBIDDEN },
    Attack { target: "/certs/server.crt",    trying: "a certificate",                         expect: FORBIDDEN },
    Attack { target: "/.uf/dev-server.json", trying: "the server's own state",                expect: FORBIDDEN },
];

#[test]
fn every_attack_in_the_corpus_is_refused() {
    let fixture = Fixture::new();
    for attack in CORPUS {
        let response = fixture.get(attack.target);
        assert!(
            !response.status.is_success(),
            "{} ({}) was SERVED",
            attack.target,
            attack.trying
        );
        assert_eq!(
            response.status, attack.expect,
            "{} ({}) was refused with the wrong status",
            attack.target, attack.trying
        );
        assert!(
            response.body.is_empty(),
            "{} ({}) returned a body",
            attack.target,
            attack.trying
        );
    }
}

#[test]
fn no_attack_in_the_corpus_leaks_the_secret_marker() {
    let fixture = Fixture::new();
    for attack in CORPUS {
        let bytes = fixture.get(attack.target).to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET),
            "{} ({}) leaked the marker",
            attack.target,
            attack.trying
        );
    }
}

#[test]
fn the_corpus_covers_every_documented_cve_row() {
    // The four rows in `docs/security.md` each need at least one entry, spelled
    // exactly as the advisory spells it.
    for required in [
        "/.env?raw??",
        "/.env?import&raw??",
        "/.env?inline",
        "http://evil.test/.env",
        "/.env\\",
        "/@fs/etc/passwd",
    ] {
        assert!(
            CORPUS.iter().any(|attack| attack.target == required),
            "the corpus lost its {required} row"
        );
    }
}

#[test]
#[cfg(unix)]
fn a_symlink_out_of_the_root_is_refused() {
    let fixture = Fixture::new();
    for target in ["/passwd.js", "/etc/passwd", "/etc/hosts"] {
        let response = fixture.get(target);
        assert_eq!(response.status, Status::Forbidden, "{target} was served");
        assert!(response.body.is_empty());
    }
}

#[test]
#[cfg(unix)]
fn a_symlink_to_a_denied_file_is_refused() {
    // The link name is innocent; only the canonical path is not. The decision
    // runs on the canonical path, so the link does not launder anything.
    let fixture = Fixture::new();
    let response = fixture.get("/innocent.js");
    assert_eq!(response.status, Status::Forbidden);
    assert!(!String::from_utf8_lossy(&response.to_bytes()).contains(SECRET));
}

#[test]
fn a_rebinding_host_header_is_refused() {
    let fixture = Fixture::new();
    for host in [
        "evil.test",
        "evil.test:5173",
        "127.0.0.1.evil.test",
        "localhost.evil.test",
    ] {
        let response =
            fixture.request(&format!("GET /index.html HTTP/1.1\r\nhost: {host}\r\n\r\n"));
        assert_eq!(response.status, Status::Forbidden, "{host} was accepted");
        assert!(response.body.is_empty());
    }
}

#[test]
fn a_request_without_a_host_header_is_refused() {
    let fixture = Fixture::new();
    let response = fixture.request("GET /index.html HTTP/1.1\r\n\r\n");
    assert_eq!(response.status, Status::BadRequest);
}

#[test]
fn a_preflight_from_a_foreign_origin_is_refused() {
    let fixture = Fixture::new();
    for origin in ["http://evil.test", "https://evil.test", "null"] {
        let response = fixture.request(&format!(
            "OPTIONS /index.html HTTP/1.1\r\nhost: localhost\r\norigin: {origin}\r\n\r\n"
        ));
        assert_eq!(response.status, Status::Forbidden, "{origin} was accepted");
    }
}

#[test]
fn a_foreign_origin_still_cannot_read_a_denied_file() {
    let fixture = Fixture::new();
    let response = fixture
        .request("GET /.env HTTP/1.1\r\nhost: localhost\r\norigin: http://evil.test\r\n\r\n");
    assert_eq!(response.status, Status::Forbidden);
    assert!(!String::from_utf8_lossy(&response.to_bytes()).contains(SECRET));
}

#[test]
fn no_inbound_header_can_reach_a_denied_file() {
    // CVE-2025-29927's class: a header that participates in dispatch. None of
    // these changes anything, because none of them is retained.
    let fixture = Fixture::new();
    let plain = fixture.get("/index.html");
    let loaded = fixture.request(concat!(
        "GET /index.html HTTP/1.1\r\n",
        "host: localhost:5173\r\n",
        "x-middleware-subrequest: middleware:middleware:middleware:middleware:middleware\r\n",
        "x-forwarded-host: evil.test\r\n",
        "x-forwarded-proto: https\r\n",
        "x-forwarded-for: 10.0.0.1\r\n",
        "x-original-url: /.env\r\n",
        "x-rewrite-url: /.env\r\n",
        "x-http-method-override: POST\r\n",
        "x-uf-loader: raw\r\n",
        "x-uf-root: /\r\n",
        "authorization: Bearer nonsense\r\n",
        "\r\n"
    ));
    assert_eq!(plain, loaded);
    assert_eq!(loaded.status, Status::Ok);
}

// --- the positive path ------------------------------------------------------

#[test]
fn ordinary_project_files_are_still_served() {
    let fixture = Fixture::new();
    for (target, expected) in [
        ("/", "<!doctype html>\n"),
        ("/index.html", "<!doctype html>\n"),
        ("/app/main.js", "export default 1;\n"),
        ("/app/main.js?import", "export default 1;\n"),
        ("/app/main.js?import&t=1700000000", "export default 1;\n"),
        ("/nested/deep/mod.js", "deep\n"),
        ("/a%20file.js", "spaced\n"),
        ("/caf%C3%A9.js", "unicode\n"),
        ("/environment.js", "not a secret\n"),
        ("/nested/../app/main.js", "export default 1;\n"),
        ("/./app/./main.js", "export default 1;\n"),
        ("/app//main.js", "export default 1;\n"),
        ("/app%5Cmain.js", "export default 1;\n"),
    ] {
        let response = fixture.get(target);
        assert_eq!(response.status, Status::Ok, "{target} was refused");
        assert_eq!(
            response.body,
            expected.as_bytes(),
            "{target} served wrongly"
        );
    }
}

#[test]
fn a_legitimate_loader_query_selects_a_loader_and_still_serves() {
    let fixture = Fixture::new();
    let response = fixture.get("/app/logo.svg?raw");
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.loader, Loader::Raw.as_str());
    assert_eq!(response.body, b"<svg/>\n");
}

#[test]
fn the_health_endpoint_answers() {
    let fixture = Fixture::new();
    let response = fixture.get("/__uf/health");
    assert_eq!(response.status, Status::Ok);
    assert_eq!(response.body, br#"{"status":"ok","engine":"uf-native"}"#);
}

// --- hot module replacement -------------------------------------------------

/// Every corpus target, driven through the update path instead of the plain
/// one. The two must be indistinguishable: an update is not a second way into
/// the filesystem, it is the same `resolve_with_policy` behind a different name.
#[test]
fn an_hmr_fetch_is_refused_exactly_like_a_plain_request() {
    let fixture = Fixture::new();
    for attack in CORPUS {
        let over_hmr = uf_devserver::fetch_update(&fixture.fs, attack.target);
        let over_the_plain_path = RequestTarget::parse(attack.target)
            .map_err(AccessDenied::from)
            .and_then(|target| resolve_with_policy(&fixture.fs, &target));

        match (over_hmr, over_the_plain_path) {
            (Err(hmr), Err(plain)) => assert_eq!(
                hmr, plain,
                "{} ({}) is refused differently over the update path",
                attack.target, attack.trying
            ),
            (Ok(file), Ok(_)) => panic!(
                "{} ({}) produced a file: {}",
                attack.target,
                attack.trying,
                file.checked_path()
            ),
            (hmr, plain) => panic!(
                "{} ({}) disagrees: update path {:?}, plain path {:?}",
                attack.target,
                attack.trying,
                hmr.map(|file| file.checked_path().to_string()),
                plain.map(|file| file.checked_path().to_string()),
            ),
        }
    }
}

/// The row the bar names explicitly.
#[test]
fn an_hmr_fetch_for_a_parent_relative_env_file_is_refused() {
    let fixture = Fixture::new();
    for target in ["/../../.env", "/../.env", "/app/../../.env"] {
        let over_hmr = uf_devserver::fetch_update(&fixture.fs, target).unwrap_err();
        let over_the_plain_path = resolve_with_policy(
            &fixture.fs,
            &RequestTarget::parse(target).expect("origin-form"),
        )
        .unwrap_err();

        assert_eq!(over_hmr, over_the_plain_path, "{target}");
        assert_eq!(over_hmr, AccessDenied::Escape, "{target}");
    }
}

/// An update payload can only ever name a target the pipeline would serve, so
/// the builder refuses the paths the pipeline refuses.
#[test]
fn an_update_target_cannot_be_built_for_anything_outside_the_project() {
    for path in [
        "../.env",
        "../../.env",
        "/etc/passwd",
        "/.env",
        "",
        "app/50%AB.js",
    ] {
        assert!(
            update_target(Utf8Path::new(path), 1).is_none(),
            "{path} must have no update target"
        );
    }
}

/// Every target the builder *does* produce survives the whole HTTP surface.
#[test]
fn every_built_update_target_round_trips_through_the_server() {
    let fixture = Fixture::new();
    for (path, expected) in [
        ("app/main.js", "export default 1;\n"),
        ("a file.js", "spaced\n"),
        ("café.js", "unicode\n"),
        ("nested/deep/mod.js", "deep\n"),
    ] {
        let built = update_target(Utf8Path::new(path), 12).expect("encodable");
        let response = fixture.get(&built);
        assert_eq!(response.status, Status::Ok, "{built} was refused");
        assert_eq!(response.body, expected.as_bytes(), "{built} served wrongly");
    }
}

/// A denied file still has a spellable update target — and it still is not
/// served. The refusal is the pipeline's, not the builder's.
#[test]
fn a_built_update_target_for_a_denied_file_is_still_refused() {
    let fixture = Fixture::new();
    let built = update_target(Utf8Path::new("config/.env.local"), 1).expect("encodable");

    let response = fixture.get(&built);

    assert_eq!(response.status, Status::Forbidden);
    assert!(!String::from_utf8_lossy(&response.to_bytes()).contains(SECRET));
}

/// The reserved update target is never a file, whatever is on disk under it.
#[test]
fn the_update_target_is_not_a_file_route() {
    let fixture = Fixture::new();
    for target in [
        uf_devserver::HMR_TARGET,
        "/__uf/hmr?raw",
        "/./__uf/hmr",
        "/x/../__uf/hmr",
    ] {
        let response = fixture.get(target);
        assert!(
            !response.status.is_success() || response.body.is_empty(),
            "{target} served a body"
        );
        assert!(!String::from_utf8_lossy(&response.to_bytes()).contains(SECRET));
    }
}

/// The shipped client runtime and the server must agree about the wire.
#[test]
fn the_client_runtime_names_the_endpoint_the_server_serves() {
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uf_lib/lib/core/hmr.js")
        .canonicalize()
        .expect("the shipped client runtime exists");
    let source = std::fs::read_to_string(&runtime).expect("readable");

    assert!(
        source.contains(&format!("\"{}\"", uf_devserver::HMR_TARGET)),
        "the client runtime must open {}",
        uf_devserver::HMR_TARGET
    );
    assert!(
        source.contains(&format!("\"{}\"", uf_devserver::hmr::UPDATE_EVENT)),
        "the client runtime must listen for {}",
        uf_devserver::hmr::UPDATE_EVENT
    );
    assert!(source.starts_with("// @flow\n"));
}

#[test]
fn every_denied_fixture_file_is_readable_from_disk() {
    // Guards the guard: if a corpus row passed only because the file was never
    // created, this test is what notices.
    let fixture = Fixture::new();
    for relative in [
        ".env",
        ".env.local",
        "config/.env.local",
        ".git/config",
        "certs/server.pem",
        "certs/server.key",
        ".uf/dev-server.json",
        "..../.env",
    ] {
        let contents = std::fs::read_to_string(fixture.root.join(relative)).unwrap();
        assert!(contents.contains(SECRET), "{relative} was not planted");
    }
}
