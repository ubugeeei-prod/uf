//! A typed HTTP client for the `$route.js` table, without importing server code.

use crate::{RouteParam, RouteParamKind, ServerModule, ServerModuleKind, route_args_type};

pub(crate) fn generate_http_client(modules: &[ServerModule]) -> String {
    let handlers: Vec<_> = modules
        .iter()
        .filter(|module| module.kind == ServerModuleKind::RouteHandler)
        .collect();
    if handlers.is_empty() {
        return String::new();
    }
    let mut source = String::from(
        "\nimport { createRouteClient } from \"@uniflowed/router/http-client\";\nimport type { RouteClientOptions, RouteRequest } from \"@uniflowed/router/http-client\";\n\nexport type RequestPath = ",
    );
    source.push_str(
        &handlers
            .iter()
            .map(|module| serde_json::to_string(&module.path).unwrap())
            .collect::<Vec<_>>()
            .join(" | "),
    );
    source.push_str(";\n\nexport type RequestArgs = {\n");
    for handler in handlers {
        let params = handler
            .path
            .split('/')
            .filter_map(|segment| {
                let name = segment.strip_prefix(':')?;
                let (name, kind) = if let Some(name) = name.strip_suffix("*?") {
                    (name, RouteParamKind::OptionalCatchAll)
                } else if let Some(name) = name.strip_suffix('*') {
                    (name, RouteParamKind::CatchAll)
                } else {
                    (name, RouteParamKind::Single)
                };
                Some(RouteParam {
                    name: name.into(),
                    kind,
                })
            })
            .collect::<Vec<_>>();
        source.push_str(&format!(
            "  {}: {},\n",
            serde_json::to_string(&handler.path).unwrap(),
            route_args_type(&params)
        ));
    }
    source.push_str("};\n\nexport type RequestClient = {\n  request: <Path extends RequestPath>(path: Path, options: RouteRequest, ...params: RequestArgs[Path]) => Promise<Response>,\n};\n\nexport function createClient(options: RouteClientOptions): RequestClient {\n  const send = createRouteClient(options);\n  return {\n    request<Path extends RequestPath>(path: Path, options: RouteRequest, ...params: RequestArgs[Path]): Promise<Response> {\n      return send(buildRoute(path, ...params), options);\n    },\n  };\n}\n");
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_paths_and_parameters_form_a_separate_closed_client_table() {
        let source = generate_http_client(&[
            ServerModule {
                path: "/api/users/:id".into(),
                file: "app/api/users/[id]/$route.js".into(),
                kind: ServerModuleKind::RouteHandler,
            },
            ServerModule {
                path: "/api/files/:path*".into(),
                file: "app/api/files/[...path]/$route.js".into(),
                kind: ServerModuleKind::RouteHandler,
            },
            ServerModule {
                path: "/api/docs/:rest*?".into(),
                file: "app/api/docs/[[...rest]]/$route.js".into(),
                kind: ServerModuleKind::RouteHandler,
            },
            ServerModule {
                path: "/private".into(),
                file: "app/private/$middleware.js".into(),
                kind: ServerModuleKind::Middleware,
            },
        ]);
        assert!(source.contains("\"/api/users/:id\""));
        assert!(source.contains("id: string"));
        assert!(source.contains("path: $ReadOnlyArray<string>"));
        assert!(source.contains("rest: $ReadOnlyArray<string>"), "{source}");
        assert!(!source.contains("/private"));
        assert!(!source.contains("$route.js"));
        let parsed = uf_fmt::format_source(&source, &uf_config::FmtConfig::default());
        assert!(parsed.is_ok(), "{parsed:?}");
    }
}
