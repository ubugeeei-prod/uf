//! OS association files and native route tables share the same explicit claims.
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::UniflowedConfig;
use uf_router::{Route, RouteTarget, discover_routes_for_target};

pub(crate) fn write(
    root: &Utf8Path,
    out: &Utf8Path,
    config: &UniflowedConfig,
    web: &[Route],
) -> Result<Vec<Utf8PathBuf>> {
    let Some(links) = &config.app.router.native_links else {
        return Ok(Vec::new());
    };
    for target in [RouteTarget::Web, RouteTarget::Ios, RouteTarget::Android] {
        let native;
        let routes = if target == RouteTarget::Web {
            web
        } else {
            native = discover_routes_for_target(root, config, target)?;
            &native
        };
        for claim in &links.routes {
            if !routes.iter().any(|route| route.path == *claim) {
                bail!("app.router.nativeLinks.routes claims {claim}, which has no {target:?} page");
            }
        }
    }
    let wildcard = |route: &str| {
        route
            .split('/')
            .map(|part| if part.starts_with(':') { "*" } else { part })
            .collect::<Vec<_>>()
            .join("/")
    };
    let patterns: Vec<_> = links
        .routes
        .iter()
        .flat_map(|route| {
            // An optional catch-all serves the path above it too, and `/docs/*`
            // does not match `/docs`, so that one is claimed on its own.
            let parent = route
                .rsplit_once('/')
                .filter(|(_, last)| last.starts_with(':') && last.ends_with("*?"))
                .map(|(parent, _)| match parent {
                    "" => "/".to_owned(),
                    parent => wildcard(parent),
                });
            std::iter::once(wildcard(route)).chain(parent)
        })
        .collect();
    let components: Vec<_> = patterns
        .iter()
        .map(|pattern| json!({ "/": pattern }))
        .collect();
    let apple = json!({ "applinks": { "details": [{ "appIDs": links.ios_app_ids, "components": components }] } });
    let android = json!([{
        "relation": ["delegate_permission/common.handle_all_urls"],
        "target": { "namespace": "android_app", "package_name": links.android_package,
                    "sha256_cert_fingerprints": links.android_sha256 },
        "relation_extensions": { "delegate_permission/common.handle_all_urls": {
            "dynamic_app_link_components": components
        } }
    }]);
    let directory = out.join(".well-known");
    std::fs::create_dir_all(&directory).with_context(|| format!("failed to create {directory}"))?;
    let mut files = Vec::new();
    for (name, value) in [
        ("apple-app-site-association", apple),
        ("assetlinks.json", android),
    ] {
        let file = directory.join(name);
        if file.exists() {
            bail!(
                "{file} already exists: remove the public copy or app.router.nativeLinks so there is one owner of the association files"
            );
        }
        std::fs::write(
            &file,
            format!("{}\n", serde_json::to_string_pretty(&value)?),
        )?;
        files.push(file);
    }
    Ok(files)
}
