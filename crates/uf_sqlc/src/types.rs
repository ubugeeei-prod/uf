//! SQL types to Flow types and runtime codecs, per engine.
//!
//! The table in `docs/sqlc.md` ("Exact types") is this module, in prose. A
//! change to one is a change to the other.

use crate::options::{Int8, Numeric, Options, OverrideType, SqliteInteger, Timestamptz};
use crate::proto::Column;

/// The engines sqlc supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Postgresql,
    Mysql,
    Sqlite,
}

impl Engine {
    /// From `Settings.engine`.
    ///
    /// # Errors
    ///
    /// For an engine this generator does not know.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "postgresql" => Ok(Self::Postgresql),
            "mysql" => Ok(Self::Mysql),
            "sqlite" => Ok(Self::Sqlite),
            other => Err(uf_infra::into_string(compact_str::format_compact!(
                "unsupported sqlc engine `{other}`"
            ))),
        }
    }

    /// The codec module's subpath, and the name generated code imports it as.
    #[must_use]
    pub fn module(self) -> &'static str {
        match self {
            Self::Postgresql => "postgresql",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }

    /// The local name of the codec module in generated code.
    #[must_use]
    pub fn alias(self) -> &'static str {
        match self {
            Self::Postgresql => "pg",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }
}

/// An override, resolved: where its bindings come from, relative to the
/// generated file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Resolved {
    pub specifier: String,
    pub name: String,
    pub decode: String,
    pub encode: Option<String>,
}

/// How generated code gets from a column's wire value to its Flow value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Codec {
    /// An export of the engine's codec module: `pg.int8`.
    Export(&'static str),
    /// A codec the engine module builds for a SQL type name: `pg.text("uuid")`.
    Factory(&'static str, String),
    /// A generated enum's codec, exported from `models.js`.
    Enum { codec: String, type_name: String },
    /// A PostgreSQL array of the inner codec.
    Array(Box<Codec>),
    /// An override's functions around the default codec.
    Mapped { base: Box<Codec>, over: Resolved },
}

/// A column's type, decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    /// The Flow type of a non-null value.
    pub flow: String,
    pub codec: Codec,
    /// Whether the value may be `NULL`.
    pub nullable: bool,
    /// A type sqlc could not resolve: `mixed`, which already includes `null`.
    pub unknown: bool,
}

impl Mapped {
    /// The Flow type, with `| null` when the column is nullable.
    #[must_use]
    pub fn flow_type(&self) -> String {
        if self.nullable && !self.unknown {
            uf_infra::into_string(compact_str::format_compact!("{} | null", self.flow))
        } else {
            self.flow.clone()
        }
    }
}

/// A generated enum, for lookups by SQL name.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub schema: String,
    pub sql_name: String,
    pub type_name: String,
    pub values_name: String,
    pub codec_name: String,
    pub values: Vec<String>,
}

/// What a type mapping needs to know about the request.
pub struct Context<'a> {
    pub engine: Engine,
    pub options: &'a Options,
    pub default_schema: &'a str,
    pub enums: &'a [EnumInfo],
    /// Overrides with their specifiers already made relative to the output.
    pub overrides: &'a [(crate::options::Override, Resolved)],
}

/// Strip a PostgreSQL type name to what matters: `pg_catalog.int8` → `int8`,
/// `character varying(20)` → `character varying`.
fn base_name(column: &Column) -> (String, String) {
    let (schema, name) = column
        .type_
        .as_ref()
        .map_or((String::new(), String::new()), |ty| {
            (ty.schema.clone(), ty.name.clone())
        });
    let lower = name.to_ascii_lowercase();
    let lower = lower
        .strip_prefix("pg_catalog.")
        .map_or(lower.clone(), str::to_owned);
    let lower = match lower.find('(') {
        Some(paren) => lower[..paren].trim_end().to_owned(),
        None => lower,
    };
    (schema, lower)
}

fn export(flow: &str, name: &'static str) -> (String, Codec) {
    (flow.to_owned(), Codec::Export(name))
}

fn factory(flow: &str, name: &'static str, sql_type: &str) -> (String, Codec) {
    (flow.to_owned(), Codec::Factory(name, sql_type.to_owned()))
}

impl Context<'_> {
    fn find_enum(&self, schema: &str, name: &str) -> Option<&EnumInfo> {
        let schema = if schema.is_empty() || schema == "pg_catalog" {
            self.default_schema
        } else {
            schema
        };
        self.enums
            .iter()
            .find(|info| info.sql_name == name && (info.schema == schema || info.schema.is_empty()))
            .or_else(|| self.enums.iter().find(|info| info.sql_name == name))
    }

    fn find_override(&self, column: &Column) -> Option<&Resolved> {
        let table = column.table.as_ref();
        let (schema, type_name) = column
            .type_
            .as_ref()
            .map_or((String::new(), String::new()), |ty| {
                (ty.schema.clone(), ty.name.clone())
            });
        let qualified_type = if schema.is_empty() {
            type_name.clone()
        } else {
            uf_infra::into_string(compact_str::format_compact!("{schema}.{type_name}"))
        };
        let name = if column.original_name.is_empty() {
            &column.name
        } else {
            &column.original_name
        };
        self.overrides.iter().find_map(|(entry, resolved)| {
            let hit = if let Some(target) = &entry.column {
                let table = table?;
                let parts: Vec<&str> = target.split('.').collect();
                match parts.as_slice() {
                    [t, c] => *t == table.name && c == name,
                    [s, t, c] => {
                        let table_schema = if table.schema.is_empty() {
                            self.default_schema
                        } else {
                            table.schema.as_str()
                        };
                        *s == table_schema && *t == table.name && c == name
                    }
                    _ => false,
                }
            } else if let Some(db_type) = &entry.db_type {
                let matches_type = db_type.eq_ignore_ascii_case(&type_name)
                    || db_type.eq_ignore_ascii_case(&qualified_type);
                matches_type
                    && entry
                        .nullable
                        .is_none_or(|nullable| nullable == !column.not_null)
            } else {
                false
            };
            hit.then_some(resolved)
        })
    }

    /// The Flow type and codec of `column`.
    ///
    /// # Errors
    ///
    /// For a column with no type at all, which sqlc does not send.
    pub fn map(&self, column: &Column) -> Mapped {
        let (schema, name) = base_name(column);
        let (flow, codec, unknown) = self.scalar(column, &schema, &name);
        let (flow, codec) = match self.find_override(column) {
            Some(over) if !unknown => (
                over.name.clone(),
                Codec::Mapped {
                    base: Box::new(codec),
                    over: over.clone(),
                },
            ),
            _ => (flow, codec),
        };
        let (flow, codec) = if column.is_array && self.engine == Engine::Postgresql && !unknown {
            let dims = column.array_dims.max(1);
            let mut flow = flow;
            let mut codec = codec;
            for _ in 0..dims {
                flow =
                    uf_infra::into_string(compact_str::format_compact!("$ReadOnlyArray<{flow}>"));
                codec = Codec::Array(Box::new(codec));
            }
            (flow, codec)
        } else {
            (flow, codec)
        };
        Mapped {
            flow,
            codec,
            nullable: !column.not_null,
            unknown,
        }
    }

    fn scalar(&self, column: &Column, schema: &str, name: &str) -> (String, Codec, bool) {
        if let Some(info) = self.find_enum(schema, name) {
            // sqlc reports a MySQL `SET` as an enum of its members, but a
            // value is any comma-joined subset of them. The column's length
            // tells the two apart: an enum is as long as its longest member,
            // a set as long as all of them joined.
            let longest = info.values.iter().map(String::len).max().unwrap_or(0);
            if self.engine == Engine::Mysql
                && usize::try_from(column.length).is_ok_and(|len| len > longest)
            {
                return (
                    "string".to_owned(),
                    Codec::Factory("text", "set".to_owned()),
                    false,
                );
            }
            return (
                info.type_name.clone(),
                Codec::Enum {
                    codec: info.codec_name.clone(),
                    type_name: info.type_name.clone(),
                },
                false,
            );
        }
        let mapped = match self.engine {
            Engine::Postgresql => self.postgresql(name),
            Engine::Mysql => self.mysql(column, name),
            Engine::Sqlite => self.sqlite(name),
        };
        match mapped {
            Some((flow, codec)) => (flow, codec, false),
            None => ("mixed".to_owned(), Codec::Export("unknown"), true),
        }
    }

    fn postgresql(&self, name: &str) -> Option<(String, Codec)> {
        Some(match name {
            "smallint" | "int2" | "smallserial" | "serial2" => export("number", "int2"),
            "integer" | "int" | "int4" | "serial" | "serial4" => export("number", "int4"),
            "oid" | "xid" | "cid" | "regclass" | "regtype" | "regproc" => export("number", "oid"),
            "bigint" | "int8" | "bigserial" | "serial8" | "xid8" => match self.options.int8 {
                Int8::Bigint => export("bigint", "int8"),
                Int8::Number => export("number", "int8AsNumber"),
                Int8::String => export("string", "int8AsString"),
            },
            "real" | "float4" => export("number", "float4"),
            "float" | "double precision" | "float8" => export("number", "float8"),
            "numeric" | "decimal" => match self.options.numeric {
                Numeric::String => export("string", "numeric"),
                Numeric::Number => export("number", "numericAsNumber"),
            },
            "money" => export("string", "money"),
            "bool" | "boolean" => export("boolean", "bool"),
            "json" | "jsonb" => export("JsonValue", "json"),
            "bytea" | "blob" => export("Uint8Array", "bytea"),
            "date" => export("string", "date"),
            "timestamp" | "timestamp without time zone" => export("string", "timestamp"),
            "timestamptz" | "timestamp with time zone" => match self.options.timestamptz {
                Timestamptz::Date => export("Date", "timestamptz"),
                Timestamptz::String => export("string", "timestamptzAsString"),
            },
            "text" | "varchar" | "character varying" | "bpchar" | "char" | "character"
            | "string" | "citext" | "name" => export("string", "string"),
            // A pseudo-type sqlc could not resolve: whatever the server sends.
            "any"
            | "anyelement"
            | "anyarray"
            | "anynonarray"
            | "anyenum"
            | "anyrange"
            | "anymultirange"
            | "anycompatible"
            | "anycompatiblearray"
            | "anycompatiblenonarray"
            | "anycompatiblerange"
            | "record"
            | "unknown"
            | "void"
            | "" => return None,
            // Every other type — `uuid`, `inet`, `interval`, `time`, ranges,
            // geometry, extensions, composites, domains — arrives as its text,
            // so a string is the one type that is always true of it.
            other => factory("string", "text", other),
        })
    }

    fn mysql(&self, column: &Column, name: &str) -> Option<(String, Codec)> {
        Some(match name {
            "tinyint" if column.length == 1 => export("boolean", "boolean"),
            "bool" | "boolean" => export("boolean", "boolean"),
            "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "year" => {
                factory("number", "integer", name)
            }
            "bigint" => match self.options.int8 {
                Int8::Bigint => export("bigint", "bigint"),
                Int8::Number => export("number", "bigintAsNumber"),
                Int8::String => export("string", "bigintAsString"),
            },
            "decimal" | "dec" | "fixed" | "numeric" => match self.options.numeric {
                Numeric::String => export("string", "decimal"),
                Numeric::Number => factory("number", "float", name),
            },
            "float" | "double" | "double precision" | "real" => factory("number", "float", name),
            "char" | "varchar" | "text" | "tinytext" | "mediumtext" | "longtext" => {
                export("string", "string")
            }
            "date" | "datetime" | "timestamp" | "time" | "set" => factory("string", "text", name),
            "binary" | "varbinary" | "blob" | "tinyblob" | "mediumblob" | "longblob" | "bit" => {
                factory("Uint8Array", "bytes", name)
            }
            "json" => export("JsonValue", "json"),
            _ => return None,
        })
    }

    fn sqlite(&self, name: &str) -> Option<(String, Codec)> {
        let integer = || match self.options.sqlite_integer {
            SqliteInteger::Number => export("number", "integer"),
            SqliteInteger::Bigint => export("bigint", "integerAsBigint"),
        };
        Some(match name {
            "integer" | "int" | "bigint" | "smallint" | "tinyint" | "mediumint" | "int2"
            | "int8" | "unsigned big int" => integer(),
            "real" | "double" | "double precision" | "doubleprecision" | "float" | "numeric"
            | "decimal" => factory("number", "real", name),
            "boolean" | "bool" => export("boolean", "boolean"),
            "blob" => export("Uint8Array", "blob"),
            "text" | "varchar" | "char" | "clob" | "nchar" | "nvarchar" | "character"
            | "varying character" | "native character" => export("string", "string"),
            "date" | "datetime" | "timestamp" | "time" => factory("string", "text", name),
            "json" | "jsonb" => export("JsonValue", "json"),
            "any" | "" => return None,
            // SQLite's own affinity rules, in its order (§3.1 of "Datatypes
            // In SQLite"). NUMERIC affinity can hold any storage class, so a
            // type that lands there is `mixed`.
            other if other.contains("int") => integer(),
            other if other.contains("char") || other.contains("clob") || other.contains("text") => {
                factory("string", "text", other)
            }
            other if other.contains("blob") => export("Uint8Array", "blob"),
            other if other.contains("real") || other.contains("floa") || other.contains("doub") => {
                factory("number", "real", other)
            }
            _ => return None,
        })
    }
}

/// An override's import specifier as seen from the generated file.
///
/// A relative `import` is relative to the sqlc config's directory, as sqlc's
/// own paths are; the generated file is in `out`, which is too.
#[must_use]
pub fn resolve_override(over: &OverrideType, out: &str) -> Resolved {
    let specifier = if over.import.starts_with("./") || over.import.starts_with("../") {
        relative(out, &over.import)
    } else {
        over.import.clone()
    };
    Resolved {
        specifier,
        name: over.name.clone(),
        decode: over.decode.clone(),
        encode: over.encode.clone(),
    }
}

fn normalize(path: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|last| last != "..") {
                    out.pop();
                } else {
                    out.push("..".to_owned());
                }
            }
            other => out.push(other.to_owned()),
        }
    }
    out
}

/// `target` (relative to the config directory) relative to `from_dir`
/// (also relative to it).
fn relative(from_dir: &str, target: &str) -> String {
    let from = normalize(from_dir);
    let to = normalize(target);
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - shared];
    parts.extend(to[shared..].iter().cloned());
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        uf_infra::into_string(compact_str::format_compact!("./{joined}"))
    }
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn override_imports_are_relative_to_the_output() {
        assert_eq!(relative("src/db", "./src/types.js"), "../types.js");
        assert_eq!(relative("src/db", "./src/db/types.js"), "./types.js");
        assert_eq!(relative(".", "./types.js"), "./types.js");
        assert_eq!(relative("gen", "../shared/ids.js"), "../../shared/ids.js");
    }
}
