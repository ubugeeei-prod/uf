//! Parser-backed, minimal config edits. Unchanged bytes keep their formatting.
use super::super::dev::config_file::outline::{self, Entry, Object, Value};
use anyhow::{Result, bail, ensure};
use serde_json::Value as Json;

pub(super) fn object(source: &str) -> Result<Object> {
    let parsed = uf_flow::validate_source(source)?;
    ensure!(
        parsed.is_ok(),
        "configuration does not parse: {:?}",
        parsed.diagnostics
    );
    outline::read(source).ok_or_else(|| anyhow::anyhow!("expected an exported object or defineConfig(object); dynamic configuration needs manual migration"))
}

fn entry<'a>(source: &str, object: &'a Object, key: &str) -> Result<Option<&'a Entry>> {
    ensure!(
        object.entries.iter().all(|e| e.key.is_some()),
        "computed keys, methods and spreads need manual migration"
    );
    let mut entries = object
        .entries
        .iter()
        .filter(|e| e.key.is_some_and(|k| k.text(source) == key));
    let first = entries.next();
    ensure!(entries.next().is_none(), "duplicate config key {key}");
    Ok(first)
}

pub(super) fn at<'a>(source: &str, root: &'a Object, path: &[&str]) -> Result<Option<&'a Value>> {
    let Some((key, rest)) = path.split_first() else {
        bail!("empty config path")
    };
    let Some(value) = entry(source, root, key)?.and_then(|e| e.value.as_ref()) else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(Some(value));
    }
    let Value::Object(object) = value else {
        bail!("{} is not a static object", key)
    };
    at(source, object, rest)
}

pub(super) fn get(source: &str, path: &[&str]) -> Result<Option<Json>> {
    let root = object(source)?;
    at(source, &root, path)?
        .map(|value| json5::from_str(&source[value.start()..value.end()]).map_err(Into::into))
        .transpose()
}

// Keep comments inside an object that is replaced, as well as surrounding ones.
fn comments(source: &str) -> String {
    let mut cursor = 0;
    let mut kept = String::new();
    for token in uf_flow::scan::tokenize(source) {
        let gap = &source[cursor..token.start];
        if gap.contains("//") || gap.contains("/*") {
            kept.push_str(gap);
            kept.push('\n');
        }
        cursor = token.end;
    }
    let gap = &source[cursor..];
    if gap.contains("//") || gap.contains("/*") {
        kept.push_str(gap);
        kept.push('\n');
    }
    kept
}

pub(super) fn put(source: &mut String, path: &[&str], value: &Json) -> Result<()> {
    put_raw(source, path, &serde_json::to_string(value)?)
}

pub(super) fn put_raw(source: &mut String, path: &[&str], value: &str) -> Result<()> {
    let (key, parents) = path
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("empty config path"))?;
    for i in 1..=parents.len() {
        if get(source, &parents[..i])?.is_none() {
            put(source, &parents[..i], &serde_json::json!({}))?;
        }
    }
    let root = object(source)?;
    let parent = if parents.is_empty() {
        &root
    } else {
        match at(source, &root, parents)? {
            Some(Value::Object(o)) => o,
            _ => bail!("{} is not an object", parents.join(".")),
        }
    };
    if let Some(existing) = entry(source, parent, key)? {
        let old = existing
            .value
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("shorthand config key {key}"))?;
        let replacement = format!("{}{value}", comments(&source[old.start()..old.end()]));
        source.replace_range(old.start()..old.end(), &replacement);
    } else {
        let indent_start = source[..parent.open].rfind('\n').map_or(0, |n| n + 1);
        let indent: String = source[indent_start..parent.open]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        source.insert_str(
            parent.open + 1,
            &format!("\n{indent}  {}: {value},", serde_json::to_string(key)?),
        );
    }
    object(source)?;
    Ok(())
}

pub(super) fn remove(source: &mut String, path: &[&str]) -> Result<()> {
    let (key, parents) = path
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("empty config path"))?;
    let root = object(source)?;
    let parent = if parents.is_empty() {
        &root
    } else {
        match at(source, &root, parents)? {
            Some(Value::Object(o)) => o,
            _ => return Ok(()),
        }
    };
    let Some(found) = entry(source, parent, key)? else {
        return Ok(());
    };
    let end = found.end();
    let comma = uf_flow::scan::tokenize(&source[end..])
        .first()
        .copied()
        .filter(|t| t.is_punct(b','));
    let kept = comments(&source[found.start()..end]);
    let start = found.start();
    if let Some(comma) = comma {
        source.replace_range(end + comma.start..end + comma.end, "");
    }
    source.replace_range(start..end, &kept);
    object(source)?;
    Ok(())
}

pub(super) fn merge(source: &mut String, path: &[&str], value: &Json) -> Result<()> {
    if let Some(old) = get(source, path)? {
        ensure!(
            old == *value,
            "{} already has a different value; both are retained for manual migration",
            path.join(".")
        );
        return Ok(());
    }
    put(source, path, value)
}
