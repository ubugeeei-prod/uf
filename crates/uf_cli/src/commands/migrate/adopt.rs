//! Conservative import of existing tool settings. Unknown settings stay named.
use super::{Plan, source};
use anyhow::{Result, ensure};
use camino::Utf8Path;
use serde_json::{Value, json};
use std::fs;

pub(super) fn plan(root: &Utf8Path) -> Result<Plan> {
    let manifest_text = fs::read_to_string(root.join("package.json"))?;
    let mut manifest: Value = serde_json::from_str(&manifest_text)?;
    ensure!(
        manifest.is_object(),
        uf_infra::cstr!("package.json must contain an object")
    );
    for field in ["scripts", "dependencies", "devDependencies"] {
        ensure!(
            manifest.get(field).is_none_or(Value::is_object),
            uf_infra::cstr!("package.json#{field} must contain an object")
        );
    }
    let before_config = fs::read_to_string(root.join("uf.config.js")).ok();
    let mut config = before_config
        .clone()
        .unwrap_or_else(|| "// @flow\nexport default {};\n".into());
    source::object(&config)?;
    let mut plan = Plan::new("migrate");
    let mut scripts = manifest
        .get("scripts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, value) in scripts.clone() {
        if uf_config::INSTALL_LIFECYCLE_SCRIPTS.contains(&name.as_str()) {
            plan.unmapped.push(uf_infra::cstr!(
                "package.json#scripts.{name}: install lifecycle hook retained; review it explicitly"
            ).into_string());
            continue;
        }
        let Some(command) = value.as_str() else {
            plan.unmapped.push(
                uf_infra::cstr!("package.json#scripts.{name}: expected a command string")
                    .into_string(),
            );
            continue;
        };
        let mapped = script(command);
        let mut candidate = config.clone();
        match source::merge(&mut candidate, &["tasks", &name], &json!(mapped)) {
            Ok(()) => {
                config = candidate;
                scripts.remove(&name);
            }
            Err(error) => plan
                .unmapped
                .push(uf_infra::cstr!("package.json#scripts.{name}: {error}").into_string()),
        }
    }
    if scripts.is_empty() {
        manifest.as_object_mut().unwrap().remove("scripts");
    } else {
        manifest["scripts"] = json!(scripts);
    }
    for name in [
        "babel.config.json",
        ".babelrc",
        ".babelrc.json",
        "babel.config.js",
        "babel.config.cjs",
        "babel.config.mjs",
        ".prettierrc",
        ".prettierrc.json",
        "prettier.config.js",
        "prettier.config.cjs",
        "prettier.config.mjs",
        ".eslintrc",
        ".eslintrc.json",
        ".eslintrc.js",
        ".eslintrc.cjs",
        "eslint.config.js",
        "eslint.config.mjs",
        "jest.config.json",
        "jest.config.js",
        "jest.config.cjs",
        "jest.config.mjs",
        "vite.config.js",
        "vite.config.mjs",
        "vite.config.ts",
    ] {
        let file = root.join(name);
        if !file.exists() {
            continue;
        }
        let text = fs::read_to_string(&file)?;
        let json = if name.ends_with(".js")
            || name.ends_with(".cjs")
            || name.ends_with(".mjs")
            || name.ends_with(".ts")
        {
            source::object(&text).map(|object| {
                let mut values = serde_json::Map::new();
                for entry in object.entries {
                    let Some(key) = entry.key else {
                        plan.unmapped.push(
                            uf_infra::cstr!("{name}: computed key, spread or method retained")
                                .into_string(),
                        );
                        continue;
                    };
                    let key = key.text(&text);
                    match entry
                        .value
                        .and_then(|v| json5::from_str::<Value>(&text[v.start()..v.end()]).ok())
                    {
                        Some(value) => {
                            values.insert(key.to_owned(), value);
                        }
                        None => plan.unmapped.push(
                            uf_infra::cstr!("{name}#{key}: dynamic expression retained")
                                .into_string(),
                        ),
                    }
                }
                Value::Object(values)
            })
        } else {
            json5::from_str::<Value>(&text).map_err(Into::into)
        };
        match json {
            Ok(value) => {
                let count = plan.unmapped.len();
                settings(name, &value, &mut config, &mut plan)?;
                let javascript = name.ends_with(".js")
                    || name.ends_with(".cjs")
                    || name.ends_with(".mjs")
                    || name.ends_with(".ts");
                if javascript {
                    plan.unmapped.push(uf_infra::cstr!("{name}: mapped static settings; keep this module until its imports and side effects have been reviewed").into_string());
                } else if count == plan.unmapped.len() {
                    plan.remove(name, text);
                }
            }
            Err(error) => plan.unmapped.push(
                uf_infra::cstr!("{name}: dynamic or unsupported configuration retained ({error})")
                    .into_string(),
            ),
        }
    }
    for key in ["babel", "prettier", "eslintConfig", "jest"] {
        if let Some(value) = manifest.get(key).cloned() {
            let count = plan.unmapped.len();
            settings(
                uf_infra::cstr!("package.json#{key}").as_str(),
                &value,
                &mut config,
                &mut plan,
            )?;
            if count == plan.unmapped.len() {
                manifest.as_object_mut().unwrap().remove(key);
            }
        }
    }
    let flow_path = root.join(".flowconfig");
    if flow_path.exists() {
        let text = fs::read_to_string(&flow_path)?;
        let count = plan.unmapped.len();
        let mut section = "";
        for line in text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        {
            if line.starts_with('[') {
                section = line;
                continue;
            }
            if section == "[options]" && line == "all=true" {
                continue;
            }
            plan.unmapped
                .push(uf_infra::cstr!(".flowconfig{section}: {line}").into_string());
        }
        if count == plan.unmapped.len() {
            plan.remove(".flowconfig", text);
        }
    }
    let cra = ["dependencies", "devDependencies"].iter().any(|field| {
        manifest
            .get(field)
            .and_then(|v| v.get("react-scripts"))
            .is_some()
    });
    if cra {
        source::merge(&mut config, &["app", "rsc"], &json!(false))?;
        plan.unmapped.push("create-react-app: review public/index.html placeholders, REACT_APP_ environment names, service workers and the application entry before uf build; source files are retained".into());
    }
    tests(root, &mut plan)?;
    if manifest.get("devDependencies").is_none() {
        manifest["devDependencies"] = json!({});
    }
    for package in ["@uniflowed/core", "@uniflowed/test", "@uniflowed/vite"] {
        if manifest
            .get("dependencies")
            .and_then(|v| v.get(package))
            .is_none()
            && manifest["devDependencies"].get(package).is_none()
        {
            manifest["devDependencies"][package] = json!(env!("CARGO_PKG_VERSION"));
        }
    }
    let after_manifest =
        uf_infra::cstr!("{}\n", serde_json::to_string_pretty(&manifest)?).into_string();
    plan.write("package.json", manifest_text, after_manifest);
    match before_config {
        Some(before) => plan.write("uf.config.js", before, config),
        None => plan.create("uf.config.js", config),
    }
    Ok(plan)
}

fn script(command: &str) -> String {
    match command.trim() {
        "vite" | "vite dev" | "react-scripts start" => "uf dev".into(),
        "vite build" | "react-scripts build" => "uf build".into(),
        "vite preview" => "uf preview".into(),
        "jest" | "react-scripts test" | "react-scripts test --watchAll=false" => "uf test".into(),
        "flow" | "flow check" => "uf check".into(),
        "eslint ." | "eslint src" => "uf lint".into(),
        "prettier --write ." => "uf fmt".into(),
        "prettier --check ." => "uf fmt --check".into(),
        _ if command.starts_with("npm run ")
            && command
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " -_:./".contains(c)) =>
        {
            command.replacen("npm run ", "uf run ", 1)
        }
        _ => command.into(),
    }
}

fn settings(name: &str, value: &Value, config: &mut String, plan: &mut Plan) -> Result<()> {
    let Some(settings) = value.as_object() else {
        plan.unmapped
            .push(uf_infra::cstr!("{name}: expected an object; retained").into_string());
        return Ok(());
    };
    for (key, value) in settings {
        let destination = if name.contains("prettier") {
            match key.as_str() {
                "tabWidth" => Some((vec!["fmt", "indentWidth"], value.clone())),
                "printWidth" => Some((vec!["fmt", "lineWidth"], value.clone())),
                "semi" => Some((vec!["fmt", "semicolons"], value.clone())),
                "singleQuote" if value.is_boolean() => Some((
                    vec!["fmt", "quotes"],
                    json!(if value == true { "single" } else { "double" }),
                )),
                _ => None,
            }
        } else if name.contains("vite.config") {
            match key.as_str() {
                "base" | "root" | "publicDir" | "build" | "server" | "preview" | "resolve"
                | "define" | "css" | "assetsInclude" | "envDir" | "envPrefix" | "optimizeDeps"
                | "ssr" => Some((vec!["vite", key], value.clone())),
                _ => None,
            }
        } else {
            None
        };
        if let Some((path, value)) = destination {
            let mut candidate = config.clone();
            match source::merge(&mut candidate, &path, &value) {
                Ok(()) => *config = candidate,
                Err(error) => plan
                    .unmapped
                    .push(uf_infra::cstr!("{name}#{key}: {error}").into_string()),
            }
            continue;
        }
        let handled = if name.contains("babel") {
            key == "presets"
                && value.as_array().is_some_and(|a| {
                    a.iter().all(|v| {
                        matches!(
                            v.as_str(),
                            Some(
                                "@babel/preset-env" | "@babel/preset-flow" | "@babel/preset-react"
                            )
                        )
                    })
                })
        } else if name.contains("jest") {
            key == "testEnvironment" && value == "node"
        } else {
            false
        };
        if !handled {
            report_keys(name, key, value, &mut plan.unmapped);
        }
    }
    Ok(())
}

fn report_keys(name: &str, key: &str, value: &Value, out: &mut Vec<String>) {
    if let Some(object) = value.as_object().filter(|v| !v.is_empty()) {
        for (child, value) in object {
            report_keys(name, uf_infra::cstr!("{key}.{child}").as_str(), value, out);
        }
    } else {
        out.push(uf_infra::cstr!("{name}#{key}: retained; no equivalent migration").into_string());
    }
}

fn tests(root: &Utf8Path, plan: &mut Plan) -> Result<()> {
    let api = [
        "test",
        "it",
        "describe",
        "expect",
        "beforeAll",
        "afterAll",
        "beforeEach",
        "afterEach",
    ];
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some("node_modules" | ".git" | ".uf" | "dist" | "build" | "coverage")
            )
        })
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if ![".test.js", ".spec.js", ".test.jsx", ".spec.jsx"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .to_string();
        let before = fs::read_to_string(entry.path())?;
        let parsed = uf_flow::validate_source(&before)?;
        if !parsed.is_ok() {
            plan.unmapped
                .push(uf_infra::cstr!("{path}: syntax needs manual migration").into_string());
            continue;
        }
        let tokens = uf_flow::scan::tokenize(&before);
        let mut used = Vec::new();
        for token in &tokens {
            if token.is_ident(&before, "jest") {
                plan.unmapped.push(
                    uf_infra::cstr!(
                        "{path}: jest mocks/timers require review against uf's uft API"
                    )
                    .into_string(),
                );
            }
            for name in &api {
                if token.is_ident(&before, name) && !used.contains(name) {
                    used.push(*name);
                }
            }
        }
        if before.contains("@uniflowed/test") {
            continue;
        }
        // Read actual import bindings. A source import unrelated to testing
        // is preserved, and an API name already bound is never shadowed.
        let mut bound = std::collections::BTreeSet::new();
        let mut jest_sources = Vec::new();
        let parsed = uf_flow::parse(&before)?;
        for statement in parsed.program.statements.iter() {
            if let uf_flow::ast::statement::StatementInner::ImportDeclaration { inner, .. } =
                &**statement
            {
                if inner.source.1.value.as_str() == "@jest/globals" {
                    let offset = |position: uf_flow::Position| {
                        before
                            .split_inclusive('\n')
                            .take(position.line.saturating_sub(1) as usize)
                            .map(str::len)
                            .sum::<usize>()
                            + position.column as usize
                    };
                    jest_sources.push(offset(inner.source.0.start)..offset(inner.source.0.end));
                }
                if let Some(default) = &inner.default {
                    bound.insert(default.identifier.name.to_string());
                }
                if let Some(specifiers) = &inner.specifiers {
                    use uf_flow::ast::statement::import_declaration::Specifier;
                    match specifiers {
                        Specifier::ImportNamespaceSpecifier((_, local)) => {
                            bound.insert(local.name.to_string());
                        }
                        Specifier::ImportNamedSpecifiers(named) => {
                            for specifier in named.iter() {
                                bound.insert(
                                    specifier
                                        .local
                                        .as_ref()
                                        .unwrap_or(&specifier.remote)
                                        .name
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
        for pair in tokens.windows(2) {
            if ["const", "let", "var", "function", "class"].contains(&pair[0].text(&before)) {
                bound.insert(pair[1].text(&before).to_owned());
            }
        }
        let mut after = before.clone();
        if !jest_sources.is_empty() {
            if tokens.iter().any(|t| t.is_ident(&before, "jest")) {
                continue;
            }
            // Replace only the source literal, never comments or string data.
            for range in jest_sources.into_iter().rev() {
                after.replace_range(range, "\"@uniflowed/test\"");
            }
        } else {
            used.retain(|name| !bound.contains(*name));
            if used.is_empty() {
                continue;
            }
            let at = tokens.first().map_or(0, |t| t.start);
            after.insert_str(
                at,
                &uf_infra::cstr!(
                    "import {{ {} }} from \"@uniflowed/test\";\n",
                    used.join(", ")
                )
                .into_string(),
            );
        }
        if uf_flow::validate_source(&after)?.is_ok() {
            plan.write(&path, before, after);
        } else {
            plan.unmapped.push(
                uf_infra::cstr!("{path}: API bindings require manual migration").into_string(),
            );
        }
    }
    Ok(())
}
