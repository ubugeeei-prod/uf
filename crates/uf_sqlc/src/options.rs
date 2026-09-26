//! The plugin's options: the `options:` map under a `codegen` entry.
//!
//! sqlc stopped forwarding its own `overrides` and `rename` to plugins
//! (`Settings` fields 5 and 6 are reserved), so those are spelled here, after
//! sqlc-gen-go's. Unknown keys are an error rather than ignored: a misspelt
//! `timestampz` that silently did nothing would keep a `Date` the author
//! thought they had turned off.

use std::collections::BTreeMap;

use serde::Deserialize;

/// How `bigint`/`int8` columns are represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Int8 {
    /// Every value, exactly.
    #[default]
    Bigint,
    /// A `number`, which throws past 2^53 instead of rounding.
    Number,
    /// The digits.
    String,
}

/// How SQLite's `INTEGER` is represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqliteInteger {
    /// A `number`, which throws past 2^53 instead of rounding.
    #[default]
    Number,
    /// A `bigint`.
    Bigint,
}

/// How `numeric`/`decimal` columns are represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Numeric {
    /// The exact decimal string.
    #[default]
    String,
    /// A `number`, which rounds.
    Number,
}

/// How `timestamptz` columns are represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Timestamptz {
    /// A `Date`, to the millisecond.
    #[default]
    Date,
    /// The server's text, to the microsecond.
    #[serde(rename = "string")]
    String,
}

/// How column and parameter names become Flow field names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Naming {
    /// `created_at` becomes `createdAt`.
    #[default]
    #[serde(rename = "camelCase")]
    CamelCase,
    /// `created_at` stays `created_at`.
    #[serde(rename = "preserve")]
    Preserve,
}

/// A Flow type an override puts in place of the default one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverrideType {
    /// The module the type and functions come from. A relative specifier is
    /// relative to the directory of the sqlc config file, like sqlc's own paths.
    pub import: String,
    /// The exported type.
    pub name: String,
    /// An exported `(value: T) => U`, from the default representation `T`.
    pub decode: String,
    /// An exported `(value: U) => T`. Without one, `U` must already be a `T`,
    /// which the checker then verifies at the generated call.
    #[serde(default)]
    pub encode: Option<String>,
}

/// One entry of `overrides`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    /// `table.column` or `schema.table.column`.
    #[serde(default)]
    pub column: Option<String>,
    /// A database type name, as sqlc reports it: `uuid`, `pg_catalog.int8`.
    #[serde(default)]
    pub db_type: Option<String>,
    /// With `db_type`: match only nullable (`true`) or non-null (`false`)
    /// columns. Absent matches both.
    #[serde(default)]
    pub nullable: Option<bool>,
    #[serde(rename = "type")]
    pub type_: OverrideType,
}

/// Everything under `options:`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Options {
    /// The module generated code imports its runtime from.
    pub runtime: Option<String>,
    pub int8: Int8,
    pub sqlite_integer: SqliteInteger,
    pub numeric: Numeric,
    pub timestamptz: Timestamptz,
    pub naming: Naming,
    /// Keep table names as they are rather than singularising them for row
    /// types (sqlc-gen-go's `emit_exact_table_names`).
    pub exact_table_names: bool,
    /// A name, as written in SQL, to the identifier it should get.
    pub rename: BTreeMap<String, String>,
    pub overrides: Vec<Override>,
}

impl Options {
    /// The runtime module specifier.
    #[must_use]
    pub fn runtime(&self) -> &str {
        self.runtime.as_deref().unwrap_or("@uniflowed/sql")
    }
}

/// Read the options; an empty or absent map is the defaults.
///
/// # Errors
///
/// When the options are not JSON or name a key or value this plugin does not know.
pub fn parse(bytes: &[u8]) -> Result<Options, String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Options::default());
    }
    let options: Options = serde_json::from_slice(bytes).map_err(|error| {
        uf_infra::into_string(compact_str::format_compact!(
            "invalid plugin options: {error}"
        ))
    })?;
    for entry in &options.overrides {
        match (&entry.column, &entry.db_type) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(
                    "invalid plugin options: an override names exactly one of `column` and `db_type`"
                        .to_owned(),
                );
            }
            (Some(_), None) if entry.nullable.is_some() => {
                return Err(
                    "invalid plugin options: `nullable` goes with `db_type`; a column already has one"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
    Ok(options)
}
