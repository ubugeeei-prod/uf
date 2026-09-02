use serde_json::json;

use crate::{compile_babel_ast_json, official_compiler_crate};

#[test]
fn official_bridge_calls_the_facebook_react_compiler_crate() {
    let file = json!({
        "type": "File",
        "program": {
            "type": "Program",
            "body": [],
            "directives": [],
            "sourceType": "module",
            "interpreter": null
        },
        "comments": [],
        "errors": []
    });
    let scope = json!({
        "scopes": [
            {
                "id": 0,
                "parent": null,
                "kind": "program",
                "bindings": {}
            }
        ],
        "bindings": [],
        "nodeToScope": {},
        "nodeToScopeEnd": {},
        "referenceToBinding": {},
        "refNodeIdToBinding": {},
        "nodeIdToScope": {},
        "programScope": 0
    });
    let options = json!({
        "shouldCompile": false,
        "enableReanimated": false,
        "isDev": false,
        "filename": "empty.js",
        "compilationMode": "infer",
        "panicThreshold": "none",
        "target": "19",
        "noEmit": true,
        "outputMode": "lint",
        "flowSuppressions": true,
        "ignoreUseNoForget": false,
        "environment": {}
    });

    let output =
        compile_babel_ast_json(&file.to_string(), &scope.to_string(), &options.to_string())
            .expect("the official compiler accepts Babel bridge payloads");

    let upstream = official_compiler_crate();
    assert_eq!(upstream.name, "react_compiler");
    assert_eq!(upstream.version, "0.1.0");
    assert_eq!(upstream.repository, "https://github.com/facebook/react");
    assert_eq!(output.result["kind"], "success");
    assert_eq!(output.result["ast"], serde_json::Value::Null);
}
