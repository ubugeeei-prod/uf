use super::*;

mod build;
mod diagnostic;
mod reachability;
mod render;
mod resolve;

fn client(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Client)
}

fn server(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Server)
}

fn actions(path: impl Into<Utf8PathBuf>) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::ServerActions)
}

/// `ROUTER_HOOKS` is written by hand, so it is held to the package it
/// describes: the same names as the hooks `@uniflowed/router` exports, and a
/// `true` exactly where `server-components.js` — what a Server Component gets
/// for the package root — implements the hook rather than refusing it or
/// handing over a client reference.
#[test]
fn the_router_hook_table_matches_the_router() {
    let router = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../npm/router");
    let read = |file: &str| {
        let path = router.join(file);
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let index = read("index.js");
    let server = read("server-components.js");

    let mut exported = exported_hooks(&index);
    exported.sort_unstable();
    let table: Vec<&str> = ROUTER_HOOKS.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        table, exported,
        "ROUTER_HOOKS and npm/router/index.js disagree"
    );
    assert!(
        ROUTER_HOOKS.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "ROUTER_HOOKS must stay sorted for its binary search"
    );

    let mut server_exported = exported_hooks(&server);
    server_exported.sort_unstable();
    assert_eq!(
        server_exported, exported,
        "the react-server entry exports a different set of hooks"
    );
    for (name, safe) in ROUTER_HOOKS {
        let implemented = server_hook_body(&server, name)
            .is_some_and(|body| !body.trim_start().starts_with("throw "));
        assert_eq!(
            implemented, *safe,
            "`{name}`: the table says server-safe = {safe}, server-components.js disagrees"
        );
    }
}

/// Names starting `use` that `source` exports as values: `export hook` and
/// the members of `export { … }` lists.
fn exported_hooks(source: &str) -> Vec<&str> {
    let mut hooks = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("export ") {
        rest = &rest[at + "export ".len()..];
        if let Some(after) = rest.strip_prefix("hook ") {
            let end = after
                .find(|character: char| !character.is_alphanumeric() && character != '_')
                .unwrap_or(after.len());
            hooks.push(&after[..end]);
        } else if let Some(after) = rest.strip_prefix('{') {
            let list = &after[..after.find('}').expect("an export list closes")];
            hooks.extend(
                list.split(',')
                    .map(str::trim)
                    .filter(|name| name.starts_with("use")),
            );
        }
    }
    hooks
}

/// The body of `export hook <name>` in `source`, from just inside its brace.
fn server_hook_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let at = source.find(&format!("export hook {name}("))?;
    let body = &source[at..];
    Some(&body[body.find("{\n")? + 1..])
}
