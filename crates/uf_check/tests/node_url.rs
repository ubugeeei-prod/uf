//! Node's `pathToFileURL` returns a URL with a required `href`, not the
//! optional fields of the legacy `url.format` object.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, check_sources};

#[test]
fn path_to_file_url_has_a_required_href() {
    let source = r#"// @flow
import { pathToFileURL, fileURLToPath } from "node:url";
const url = pathToFileURL("/tmp/example.js");
export const href: string = url.href;
export const path: string = fileURLToPath(url);
export const wrong: number = url.href;
"#;
    let report = check_sources(
        &[Source::new("node_url.js", source)],
        &[],
        &CheckLimits::default().without_timeout(),
    )
    .expect("the checker runs");
    let actual: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.primary.start.line, diagnostic.code))
        .collect();
    assert_eq!(actual, [(6, Some("incompatible-type"))]);
}
