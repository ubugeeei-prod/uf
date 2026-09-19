//! Native association declarations are explicit and validated before any command runs.
use camino::Utf8Path;

use crate::{ConfigError, RouterConfig};

pub(crate) fn check(path: &Utf8Path, router: &RouterConfig) -> Result<(), ConfigError> {
    let Some(links) = &router.native_links else {
        return Ok(());
    };
    let refuse = |reason: &str| ConfigError::Parse {
        path: path.to_path_buf(),
        message: format!("app.router.nativeLinks: {reason}"),
    };
    if !router.base_path.is_empty() {
        return Err(refuse(
            "association files must be deployed at the origin root; basePath must be empty",
        ));
    }
    if links.origins.is_empty() || links.origins.iter().any(|origin| !is_origin(origin)) {
        return Err(refuse(
            "origins must contain HTTPS DNS origins, without credentials, paths, queries or non-default ports",
        ));
    }
    if links.routes.is_empty()
        || links.routes.iter().any(|route| {
            !route.starts_with('/')
                || route.starts_with("//")
                || route.contains(['*', '?', '#', '\\', '%'])
                || route.split('/').any(|part| matches!(part, "." | ".."))
        })
    {
        return Err(refuse(
            "routes must explicitly name static or parameterized route paths; catch-alls are not claimed",
        ));
    }
    if links.ios_app_ids.is_empty()
        || links.ios_app_ids.iter().any(|id| {
            let Some((team, bundle)) = id.split_once('.') else {
                return true;
            };
            team.len() != 10
                || !team
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                || !identifier(bundle)
        })
    {
        return Err(refuse(
            "iosAppIds must contain TEAMID.bundle.identifier with a ten-character Apple team identifier",
        ));
    }
    if !identifier(&links.android_package) {
        return Err(refuse(
            "androidPackage must be the application's dotted package identifier",
        ));
    }
    if links.android_sha256.is_empty()
        || links.android_sha256.iter().any(|fingerprint| {
            let bytes: Vec<_> = fingerprint.split(':').collect();
            bytes.len() != 32
                || bytes
                    .iter()
                    .any(|byte| byte.len() != 2 || !byte.bytes().all(|c| c.is_ascii_hexdigit()))
        })
    {
        return Err(refuse(
            "androidSha256 must contain SHA-256 fingerprints of the production app-signing certificates (32 colon-separated hexadecimal bytes)",
        ));
    }
    Ok(())
}

fn identifier(value: &str) -> bool {
    value.contains('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        })
}

fn is_origin(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let Some(host) = lower.strip_prefix("https://") else {
        return false;
    };
    let host = host.strip_suffix('/').unwrap_or(host);
    let host = host.strip_suffix(":443").unwrap_or(host);
    !host.is_empty()
        && host.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeLinksConfig;

    #[test]
    fn refuses_incomplete_identities_and_ambiguous_origins() {
        let mut router = RouterConfig::default();
        router.native_links = Some(NativeLinksConfig {
            origins: vec!["https://example.com".into()],
            routes: vec!["/users/:id".into()],
            ios_app_ids: vec!["ABCDE12345.com.example.app".into()],
            android_package: "com.example.app".into(),
            android_sha256: vec![("AA:".repeat(31) + "AA").into()],
        });
        assert!(check(Utf8Path::new("uf.config.js"), &router).is_ok());
        for origin in [
            "http://example.com",
            "https://user@example.com",
            "https://example.com/path",
            "https://example.com:8443",
            "https://example.com?query",
        ] {
            router.native_links.as_mut().unwrap().origins = vec![origin.into()];
            assert!(
                check(Utf8Path::new("uf.config.js"), &router).is_err(),
                "{origin}"
            );
        }
        router.native_links.as_mut().unwrap().origins = vec!["https://EXAMPLE.com:443/".into()];
        router.native_links.as_mut().unwrap().android_sha256.clear();
        assert!(
            check(Utf8Path::new("uf.config.js"), &router)
                .unwrap_err()
                .to_string()
                .contains("androidSha256")
        );
    }
}
