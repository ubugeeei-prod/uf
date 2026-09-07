use serde_json::json;
use sha2::Sha512;

use super::*;

const NAME: &str = "@uniflowed/core";
const VERSION: &str = "1.2.3";

/// The tarball everything below is nominally about.
fn tarball() -> Vec<u8> {
    b"the bytes that were published".to_vec()
}

fn integrity_of(bytes: &[u8]) -> String {
    format!("sha512-{}", BASE64.encode(Sha512::digest(bytes)))
}

fn sha512_hex(bytes: &[u8]) -> String {
    hex(&Sha512::digest(bytes))
}

fn statement(subject_name: &str, digest: &str) -> Value {
    json!({
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [{ "name": subject_name, "digest": { "sha512": digest } }],
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "buildDefinition": {
                "buildType": "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1",
                "externalParameters": {
                    "workflow": {
                        "ref": "refs/heads/main",
                        "repository": "https://github.com/ubugeeei-prod/uf",
                        "path": ".github/workflows/publish.yml"
                    }
                },
                "resolvedDependencies": [{
                    "uri": "git+https://github.com/ubugeeei-prod/uf@refs/heads/main",
                    "digest": { "gitCommit": "0123456789abcdef0123456789abcdef01234567" }
                }]
            }
        }
    })
}

/// A bundle whose transparency-log entry agrees with its payload, unless the
/// caller asks for one that does not.
fn bundle_of(statement: &Value, logged_payload: Option<&Value>) -> Value {
    let payload = serde_json::to_vec(statement).expect("statement");
    let logged = logged_payload.map_or_else(
        || payload.clone(),
        |value| serde_json::to_vec(value).expect("statement"),
    );
    let body = json!({
        "apiVersion": "0.0.1",
        "kind": "dsse",
        "spec": {
            "payloadHash": { "algorithm": "sha256", "value": hex(&Sha256::digest(&logged)) }
        }
    });
    json!({
        "mediaType": "application/vnd.dev.sigstore.bundle+json;version=0.2",
        "verificationMaterial": {
            "tlogEntries": [{
                "logIndex": "128506224",
                "canonicalizedBody": BASE64.encode(serde_json::to_vec(&body).expect("body"))
            }]
        },
        "dsseEnvelope": {
            "payloadType": IN_TOTO_PAYLOAD_TYPE,
            "payload": BASE64.encode(&payload),
            "signatures": [{ "sig": "MEUCIQD-not-checked-by-uf", "keyid": "" }]
        }
    })
}

fn document(bundle: Value) -> Value {
    json!({
        "package": { "name": NAME, "version": VERSION },
        "attestations": [
            { "predicateType": "https://slsa.dev/provenance/v1", "bundle": bundle }
        ]
    })
}

fn artifact(integrity: &str) -> Artifact<'_> {
    Artifact {
        name: NAME,
        version: VERSION,
        integrity,
    }
}

#[test]
fn an_attestation_about_this_tarball_is_read_and_reports_where_it_was_built() {
    let integrity = integrity_of(&tarball());
    let bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );

    let outcome =
        check_document(&document(bundle), artifact(&integrity), "the test").expect("checked");

    let Outcome::Attested(provenance) = outcome else {
        panic!("expected an attestation, got {outcome:?}");
    };
    assert_eq!(provenance.predicate_type, "https://slsa.dev/provenance/v1");
    assert_eq!(
        provenance.source_repository.as_deref(),
        Some("https://github.com/ubugeeei-prod/uf")
    );
    assert_eq!(
        provenance.build_entry.as_deref(),
        Some(".github/workflows/publish.yml")
    );
    assert_eq!(
        provenance.source_revision.as_deref(),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
    assert_eq!(
        provenance.transparency,
        Transparency::Logged { index: 128_506_224 }
    );
    assert_eq!(
        provenance.origin().as_deref(),
        Some("https://github.com/ubugeeei-prod/uf (.github/workflows/publish.yml)")
    );
}

/// The attack: the tarball is swapped and the attestation beside it is not.
///
/// This is the case an integrity hash cannot see on its own — a registry that
/// serves a different tarball under the same version — and the one the whole
/// module is for.
#[test]
fn an_attestation_about_another_tarball_is_refused_and_prints_both_digests() {
    let attested = statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball()));
    let swapped = integrity_of(b"the bytes an attacker published instead");

    let error = check_document(
        &document(bundle_of(&attested, None)),
        artifact(&swapped),
        "the test",
    )
    .expect_err("a mismatch must not pass");

    assert!(
        matches!(error, ProvenanceError::DigestMismatch { .. }),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains(&sha512_hex(&tarball())), "{message}");
    assert!(message.contains("Do not install this"), "{message}");
}

/// The other half of the same attack: the *attestation* is edited so its
/// subject digest matches the tarball the attacker swapped in. The Rekor entry
/// is a real one, copied unchanged, and no longer describes the statement.
#[test]
fn a_statement_edited_under_a_genuine_log_entry_is_refused() {
    let honest = statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball()));
    let attacker_bytes = b"the bytes an attacker published instead";
    let forged = statement(
        "pkg:npm/%40uniflowed/core@1.2.3",
        &sha512_hex(attacker_bytes),
    );
    // The bundle carries the forged statement and the honest statement's log
    // entry, which is exactly what a copied entry looks like.
    let bundle = bundle_of(&forged, Some(&honest));

    let error = check_document(
        &document(bundle),
        artifact(&integrity_of(attacker_bytes)),
        "the test",
    )
    .expect_err("a rewritten statement must not pass");

    assert!(
        matches!(error, ProvenanceError::TransparencyMismatch { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("not the one the public log saw"),
        "{error}"
    );
}

/// An attestation lifted from a package that really has one.
#[test]
fn an_attestation_copied_from_another_package_is_refused_and_names_it() {
    let integrity = integrity_of(&tarball());
    let bundle = bundle_of(
        &statement("pkg:npm/react@18.2.0", &sha512_hex(&tarball())),
        None,
    );

    let error = check_document(&document(bundle), artifact(&integrity), "the test")
        .expect_err("another package's attestation must not pass");

    assert!(
        matches!(error, ProvenanceError::WrongSubject { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("pkg:npm/react@18.2.0"),
        "{error}"
    );
}

/// The same package, a different version. A published `1.2.2` attestation
/// says nothing about `1.2.3`.
#[test]
fn an_attestation_for_another_version_of_the_same_package_is_refused() {
    let integrity = integrity_of(&tarball());
    let bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.2", &sha512_hex(&tarball())),
        None,
    );

    let error = check_document(&document(bundle), artifact(&integrity), "the test")
        .expect_err("another version's attestation must not pass");

    assert!(
        matches!(error, ProvenanceError::WrongSubject { .. }),
        "{error:?}"
    );
}

/// An envelope with no signature is not an attestation, whatever it says.
#[test]
fn an_unsigned_envelope_is_refused_before_anything_it_claims_is_read() {
    let integrity = integrity_of(&tarball());
    let mut bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );
    bundle["dsseEnvelope"]["signatures"] = json!([]);

    let error = check_document(&document(bundle), artifact(&integrity), "the test")
        .expect_err("an unsigned bundle must not pass");

    assert!(
        matches!(error, ProvenanceError::Unsigned { .. }),
        "{error:?}"
    );
}

#[test]
fn a_payload_that_is_not_an_in_toto_statement_is_refused() {
    let integrity = integrity_of(&tarball());
    let mut bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );
    bundle["dsseEnvelope"]["payloadType"] = json!("application/json");

    let error = check_document(&document(bundle), artifact(&integrity), "the test")
        .expect_err("a foreign payload type must not pass");

    assert!(
        matches!(error, ProvenanceError::NotInToto { .. }),
        "{error:?}"
    );
}

/// npm's own publish attestation is not a provenance statement, and reading it
/// as one would report a repository nobody claimed.
#[test]
fn an_attestation_of_another_kind_is_not_read_as_provenance() {
    let integrity = integrity_of(&tarball());
    let bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );
    let document = json!({
        "attestations": [{
            "predicateType": "https://github.com/npm/attestation/tree/main/specs/publish/v0.1",
            "bundle": bundle
        }]
    });

    let outcome = check_document(&document, artifact(&integrity), "the test").expect("checked");

    assert_eq!(outcome, Outcome::Unattested);
}

#[test]
fn a_package_with_no_attestation_is_reported_rather_than_refused() {
    let integrity = integrity_of(&tarball());

    assert_eq!(
        check_document(
            &json!({ "attestations": [] }),
            artifact(&integrity),
            "npmjs"
        )
        .expect("checked"),
        Outcome::Unattested
    );
    assert_eq!(
        check_document(&json!({}), artifact(&integrity), "npmjs").expect("checked"),
        Outcome::Unattested
    );
    assert_eq!(Outcome::Unattested.label(), "no");
    assert!(!Outcome::Unattested.is_attested());
}

/// A lockfile entry with only a SHA-1 has nothing an attestation can be
/// compared against, and being unable to compare is not the same as agreeing.
#[test]
fn an_integrity_uf_cannot_compare_is_refused_rather_than_waved_through() {
    let bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );

    let error = check_document(
        &document(bundle),
        artifact("sha1-abcdefghijklmnopqrst0123456="),
        "the test",
    )
    .expect_err("an uncomparable digest must not pass");

    assert!(
        matches!(error, ProvenanceError::NoDigest { .. }),
        "{error:?}"
    );
}

#[test]
fn both_purl_spellings_of_a_scoped_name_are_the_same_package() {
    for subject in [
        "pkg:npm/%40uniflowed/core@1.2.3",
        "pkg:npm/%40uniflowed%2fcore@1.2.3",
        "pkg:npm/%40uniflowed%2Fcore@1.2.3",
        "pkg:npm/%40uniflowed/core@1.2.3?repository_url=registry.npmjs.org",
    ] {
        assert!(
            names_the_same_package(subject, NAME, VERSION),
            "{subject} should name {NAME}@{VERSION}"
        );
    }
    // And a name that merely contains the right one does not.
    for subject in [
        "pkg:npm/%40evil/uniflowed-core@1.2.3",
        "pkg:npm/core@1.2.3",
        "pkg:cargo/%40uniflowed/core@1.2.3",
        "%40uniflowed/core@1.2.3",
    ] {
        assert!(
            !names_the_same_package(subject, NAME, VERSION),
            "{subject} must not name {NAME}@{VERSION}"
        );
    }
}

#[test]
fn the_attestation_url_refuses_a_name_or_a_version_that_would_steer_it() {
    assert_eq!(
        attestation_url("https://registry.npmjs.org/", NAME, VERSION).as_deref(),
        Some("https://registry.npmjs.org/-/npm/v1/attestations/@uniflowed/core@1.2.3")
    );
    // Neither half may carry a path, a query, or an authority.
    assert_eq!(
        attestation_url("https://r.example", "../../etc", "1.0.0"),
        None
    );
    assert_eq!(
        attestation_url("https://r.example", "react", "1.0.0?x=1"),
        None
    );
    assert_eq!(
        attestation_url("https://r.example", "react", "../1.0.0"),
        None
    );
    // And the registry itself is held to the same rules as a packument read.
    assert_eq!(attestation_url("http://r.example", "react", "1.0.0"), None);
    assert_eq!(
        attestation_url("https://user:token@r.example", "react", "1.0.0"),
        None
    );
}

#[test]
fn a_transparency_entry_uf_cannot_read_proves_nothing_rather_than_everything() {
    let integrity = integrity_of(&tarball());
    let mut bundle = bundle_of(
        &statement("pkg:npm/%40uniflowed/core@1.2.3", &sha512_hex(&tarball())),
        None,
    );
    bundle["verificationMaterial"]["tlogEntries"][0]["canonicalizedBody"] = json!("not base64 !!");

    let outcome =
        check_document(&document(bundle), artifact(&integrity), "the test").expect("checked");

    let Outcome::Attested(provenance) = outcome else {
        panic!("expected an attestation, got {outcome:?}");
    };
    assert_eq!(provenance.transparency, Transparency::Absent);
}
