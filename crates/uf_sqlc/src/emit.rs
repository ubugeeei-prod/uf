//! Printing the Flow modules.
//!
//! One `models.js` with the enums and a row type per table, and one module per
//! query file (`query.sql` → `query.sql.js`) with a function per query. The
//! shapes are deliberately few and regular — an arguments type, a row type, a
//! function that hands the runtime its SQL, its encoded parameters and a row
//! decoder — so that a reader can check any one of them against its SQL, and so
//! that Flow checks every decoded field against the row type it claims.

use std::collections::{BTreeMap, BTreeSet};

use crate::names::{camel, field, is_plain_key, is_reserved, pascal, singular};
use crate::options::{Naming, Options};
use crate::placeholders::{self, Dialect};
use crate::proto::{Column, File, GenerateRequest, Identifier, Query};
use crate::signed::TOKEN;
use crate::types::{Codec, Context, Engine, EnumInfo, Mapped, Resolved, resolve_override};

/// The schemas that are PostgreSQL's own, never the application's.
const SYSTEM_SCHEMAS: &[&str] = &["pg_catalog", "information_schema"];

/// A table, named.
struct Model {
    schema: String,
    sql_name: String,
    type_name: String,
    columns: Vec<Column>,
}

/// Everything about one request that every module shares.
struct Shared<'a> {
    request: &'a GenerateRequest,
    engine: Engine,
    options: &'a Options,
    default_schema: String,
    enums: Vec<EnumInfo>,
    models: Vec<Model>,
    overrides: Vec<(crate::options::Override, Resolved)>,
}

impl Shared<'_> {
    fn context(&self) -> Context<'_> {
        Context {
            engine: self.engine,
            options: self.options,
            default_schema: &self.default_schema,
            enums: &self.enums,
            overrides: &self.overrides,
        }
    }

    fn camel_case(&self) -> bool {
        self.options.naming == Naming::CamelCase
    }

    fn renamed(&self, sql_name: &str) -> Option<&String> {
        self.options.rename.get(sql_name)
    }

    fn field_name(&self, sql_name: &str) -> String {
        self.renamed(sql_name)
            .cloned()
            .unwrap_or_else(|| field(sql_name, self.camel_case()))
    }

    fn schema_of<'b>(&'b self, identifier: &'b Identifier) -> &'b str {
        if identifier.schema.is_empty() {
            &self.default_schema
        } else {
            &identifier.schema
        }
    }

    fn model(&self, identifier: &Identifier) -> Option<&Model> {
        let schema = self.schema_of(identifier);
        self.models
            .iter()
            .find(|model| model.sql_name == identifier.name && model.schema == schema)
    }

    fn sqlc_version(&self) -> &str {
        &self.request.sqlc_version
    }
}

/// Generate every file for one `GenerateRequest`.
///
/// # Errors
///
/// When the request names an unknown engine, the options are invalid, or a
/// query cannot be generated faithfully (a placeholder with no parameter, two
/// queries that would get the same name).
pub fn generate(request: &GenerateRequest) -> Result<Vec<File>, String> {
    let settings = request.settings.as_ref().ok_or("sqlc sent no settings")?;
    let engine = Engine::parse(&settings.engine)?;
    let codegen = settings.codegen.as_ref();
    let out = codegen.map_or(".", |codegen| codegen.out.as_str());
    let option_bytes = if request.plugin_options.is_empty() {
        codegen.map_or(&[][..], |codegen| codegen.options.as_slice())
    } else {
        request.plugin_options.as_slice()
    };
    let options = crate::options::parse(option_bytes)?;
    let catalog = request.catalog.clone().unwrap_or_default();
    let default_schema = if catalog.default_schema.is_empty() {
        match engine {
            Engine::Sqlite => "main".to_owned(),
            _ => "public".to_owned(),
        }
    } else {
        catalog.default_schema.clone()
    };

    let camel_case = options.naming == Naming::CamelCase;
    let qualify = |schema: &str, name: &str| {
        if schema == default_schema || schema.is_empty() {
            name.to_owned()
        } else {
            uf_infra::into_string(uf_infra::cstr!("{schema}_{name}"))
        }
    };

    let mut enums = Vec::new();
    let mut models = Vec::new();
    for schema in &catalog.schemas {
        if SYSTEM_SCHEMAS.contains(&schema.name.as_str()) {
            continue;
        }
        for entry in &schema.enums {
            let type_name = options
                .rename
                .get(&entry.name)
                .cloned()
                .unwrap_or_else(|| pascal(&qualify(&schema.name, &entry.name)));
            let base = if camel_case {
                camel(&type_name)
            } else {
                type_name.clone()
            };
            enums.push(EnumInfo {
                schema: schema.name.clone(),
                sql_name: entry.name.clone(),
                values_name: uf_infra::into_string(uf_infra::cstr!("{base}Values")),
                codec_name: uf_infra::into_string(uf_infra::cstr!("{base}Codec")),
                type_name,
                values: entry.vals.clone(),
            });
        }
        for table in &schema.tables {
            let Some(rel) = &table.rel else { continue };
            let singular_name = if options.exact_table_names {
                rel.name.clone()
            } else {
                singular(&rel.name)
            };
            let type_name = options
                .rename
                .get(&rel.name)
                .cloned()
                .unwrap_or_else(|| pascal(&qualify(&schema.name, &singular_name)));
            models.push(Model {
                schema: if rel.schema.is_empty() {
                    schema.name.clone()
                } else {
                    rel.schema.clone()
                },
                sql_name: rel.name.clone(),
                type_name,
                columns: table.columns.clone(),
            });
        }
    }
    let mut seen = BTreeSet::new();
    for name in enums
        .iter()
        .map(|info| &info.type_name)
        .chain(models.iter().map(|model| &model.type_name))
    {
        if !seen.insert(name.clone()) {
            return Err(uf_infra::into_string(uf_infra::cstr!(
                "two tables or enums would both be called `{name}`; give one a name with `rename`"
            )));
        }
    }

    let overrides = options
        .overrides
        .iter()
        .map(|entry| (entry.clone(), resolve_override(&entry.type_, out)))
        .collect();
    let shared = Shared {
        request,
        engine,
        options: &options,
        default_schema,
        enums,
        models,
        overrides,
    };

    let mut files = Vec::new();
    if !shared.enums.is_empty() || !shared.models.is_empty() {
        files.push(File {
            name: "models.js".to_owned(),
            contents: models_file(&shared)?,
        });
    }
    let mut by_file: BTreeMap<&str, Vec<&Query>> = BTreeMap::new();
    for query in &request.queries {
        by_file
            .entry(query.filename.as_str())
            .or_default()
            .push(query);
    }
    let mut function_names = BTreeSet::new();
    for (filename, queries) in by_file {
        let mut module = Module::new(&shared, false);
        for query in queries {
            if !function_names.insert(query.name.clone()) {
                return Err(uf_infra::into_string(uf_infra::cstr!(
                    "two queries are called `{}`",
                    query.name
                )));
            }
            module.query(query).map_err(|error| {
                uf_infra::into_string(uf_infra::cstr!("{filename}: query {}: {error}", query.name))
            })?;
        }
        let stem = if filename.is_empty() {
            "queries.sql"
        } else {
            filename
        };
        files.push(File {
            name: uf_infra::into_string(uf_infra::cstr!("{stem}.js")),
            contents: module.finish(uf_infra::cstr!("from `{stem}`").as_str()),
        });
    }
    Ok(files)
}

/// A JavaScript string literal for `text`: double-quoted on one line, or a
/// template literal when the SQL spans lines, so that it reads as it was
/// written.
fn js_string(text: &str) -> String {
    if text.contains('\n') && !text.contains('\r') {
        let mut out = String::from("`");
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '`' => out.push_str("\\`"),
                '\\' => out.push_str("\\\\"),
                '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
                other => out.push(other),
            }
        }
        out.push('`');
        return out;
    }
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            ch if (ch as u32) < 0x20 => uf_infra::append!(out, "\\u{:04x}", ch as u32),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn key(name: &str) -> String {
    if is_plain_key(name) {
        name.to_owned()
    } else {
        js_string(name)
    }
}

fn doc(lines: &[String], indent: &str) -> String {
    let lines: Vec<&str> = lines.iter().flat_map(|line| line.lines()).collect();
    if lines.iter().all(|line| line.trim().is_empty()) {
        return String::new();
    }
    let mut out = uf_infra::into_string(uf_infra::cstr!("{indent}/**\n"));
    for line in lines {
        let line = line.replace("*/", "*\\/");
        let line = line.trim_end();
        if line.is_empty() {
            uf_infra::append!(out, "{indent} *\n");
        } else {
            uf_infra::append!(out, "{indent} * {line}\n");
        }
    }
    uf_infra::append!(out, "{indent} */\n");
    out
}

fn header(shared: &Shared<'_>, what: &str) -> String {
    let version = shared.sqlc_version();
    let with = if version.is_empty() {
        String::new()
    } else {
        uf_infra::into_string(uf_infra::cstr!(" with sqlc {version}"))
    };
    uf_infra::into_string(uf_infra::cstr!(
        "/**\n * Generated by uf's sqlc plugin {what}{with}.\n * Do not edit: change the SQL and run `uf sqlc generate`.\n *\n * @generated {TOKEN}\n * @flow\n */\n"
    ))
}

fn models_file(shared: &Shared<'_>) -> Result<String, String> {
    let mut module = Module::new(shared, true);
    let alias = shared.engine.alias();
    for info in &shared.enums {
        let values: Vec<String> = info.values.iter().map(|value| js_string(value)).collect();
        let union = if values.is_empty() {
            "empty".to_owned()
        } else {
            values.join(" | ")
        };
        uf_infra::append!(
            module.body,
            "/** The `{}` enum. */\nexport type {} = {union};\n\nexport const {}: $ReadOnlyArray<{}> = [{}];\n\nexport const {}: {alias}.Codec<{}> = {alias}.enumeration({}, {});\n\n",
            info.sql_name,
            info.type_name,
            info.values_name,
            info.type_name,
            values.join(", "),
            info.codec_name,
            info.type_name,
            js_string(&info.sql_name),
            info.values_name,
        );
        module.uses_codecs = true;
        module.taken.insert(info.type_name.clone());
        module.taken.insert(info.values_name.clone());
        module.taken.insert(info.codec_name.clone());
    }
    let context = shared.context();
    for model in &shared.models {
        module.taken.insert(model.type_name.clone());
        let table_label = if model.schema == shared.default_schema {
            model.sql_name.clone()
        } else {
            uf_infra::into_string(uf_infra::cstr!("{}.{}", model.schema, model.sql_name))
        };
        let mut fields = Vec::new();
        let mut names = BTreeSet::new();
        for column in &model.columns {
            let mapped = context.map(&with_table(column, &model.schema, &model.sql_name));
            let name = unique(&mut names, shared.field_name(&column.name));
            let flow = module.type_of(&mapped);
            let comment = doc(std::slice::from_ref(&column.comment), "  ");
            fields.push(uf_infra::into_string(uf_infra::cstr!(
                "{comment}  readonly {}: {flow},\n",
                key(&name)
            )));
        }
        module
            .body
            .push_str(uf_infra::cstr!("/** A row of `{table_label}`. */\n").as_str());
        uf_infra::append!(
            module.body,
            "export type {} = {{|\n{}|}};\n\n",
            model.type_name,
            fields.concat()
        );
    }
    Ok(module.finish("from the schema"))
}

/// `column` as it appears in a table, with the table filled in so that a
/// `column` override can find it.
fn with_table(column: &Column, schema: &str, table: &str) -> Column {
    let mut column = column.clone();
    if column.table.is_none() {
        column.table = Some(Identifier {
            catalog: String::new(),
            schema: schema.to_owned(),
            name: table.to_owned(),
        });
    }
    column
}

fn unique(names: &mut BTreeSet<String>, name: String) -> String {
    if names.insert(name.clone()) {
        return name;
    }
    for n in 2.. {
        let candidate = uf_infra::into_string(uf_infra::cstr!("{name}{n}"));
        if names.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("the integers do not run out")
}

/// Every identifier-shaped word in `text`, including those in strings and
/// comments — an over-approximation, which can only keep an import that was
/// not needed, never drop one that was.
fn identifiers(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$'))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A row field and how to decode it.
struct RowField {
    name: String,
    flow: String,
    decode: String,
}

/// One generated module and what it has to import.
struct Module<'a> {
    shared: &'a Shared<'a>,
    is_models: bool,
    alias: &'static str,
    uses_sql: bool,
    uses_codecs: bool,
    runtime_types: BTreeSet<&'static str>,
    model_types: BTreeSet<String>,
    model_values: BTreeSet<String>,
    foreign: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    hoisted: Vec<(String, String)>,
    taken: BTreeSet<String>,
    body: String,
}

impl<'a> Module<'a> {
    fn new(shared: &'a Shared<'a>, is_models: bool) -> Self {
        let alias = shared.engine.alias();
        let taken = [
            "sql",
            alias,
            "Queryable",
            "JsonValue",
            "ExecResult",
            "CopyPlan",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        Self {
            shared,
            is_models,
            alias,
            uses_sql: false,
            uses_codecs: false,
            runtime_types: BTreeSet::new(),
            model_types: BTreeSet::new(),
            model_values: BTreeSet::new(),
            foreign: BTreeMap::new(),
            hoisted: Vec::new(),
            taken,
            body: String::new(),
        }
    }

    /// Claim a module-level name for something the SQL named.
    fn claim(&mut self, name: &str) -> Result<(), String> {
        if !self.taken.insert(name.to_owned()) {
            return Err(uf_infra::into_string(uf_infra::cstr!(
                "the generated name `{name}` is already used in this module; rename the query or the table"
            )));
        }
        Ok(())
    }

    fn slug(codec: &Codec) -> String {
        match codec {
            Codec::Export(name) => (*name).to_owned(),
            Codec::Factory(_, sql_type) => sql_type.clone(),
            Codec::Enum { type_name, .. } => type_name.clone(),
            Codec::Array(inner) => {
                uf_infra::into_string(uf_infra::cstr!("{}_array", Self::slug(inner)))
            }
            Codec::Mapped { over, .. } => over.name.clone(),
        }
    }

    fn hoist(&mut self, slug: &str, expression: String) -> String {
        if let Some((name, _)) = self
            .hoisted
            .iter()
            .find(|(_, existing)| *existing == expression)
        {
            return name.clone();
        }
        let base = uf_infra::into_string(uf_infra::cstr!("{}Codec", camel(slug)));
        let mut name = base.clone();
        let mut n = 2;
        while self.taken.contains(&name) {
            name = uf_infra::into_string(uf_infra::cstr!("{base}{n}"));
            n += 1;
        }
        self.taken.insert(name.clone());
        self.hoisted.push((name.clone(), expression));
        name
    }

    fn foreign_value(&mut self, specifier: &str, name: &str) {
        self.foreign
            .entry(specifier.to_owned())
            .or_default()
            .1
            .insert(name.to_owned());
    }

    fn foreign_type(&mut self, specifier: &str, name: &str) {
        self.foreign
            .entry(specifier.to_owned())
            .or_default()
            .0
            .insert(name.to_owned());
    }

    /// The expression for `codec`, hoisting what should be built once.
    fn codec(&mut self, codec: &Codec) -> String {
        self.uses_codecs = true;
        let alias = self.alias;
        match codec {
            Codec::Export(name) => uf_infra::into_string(uf_infra::cstr!("{alias}.{name}")),
            Codec::Factory(factory, sql_type) => self.hoist(
                sql_type,
                uf_infra::into_string(uf_infra::cstr!(
                    "{alias}.{factory}({})",
                    js_string(sql_type)
                )),
            ),
            Codec::Enum { codec, .. } => {
                if !self.is_models {
                    self.model_values.insert(codec.clone());
                }
                codec.clone()
            }
            Codec::Array(inner) => {
                let inner_ref = self.codec(inner);
                self.hoist(
                    &Self::slug(codec),
                    uf_infra::into_string(uf_infra::cstr!("{alias}.array({inner_ref})")),
                )
            }
            Codec::Mapped { base, over } => {
                let base_ref = self.codec(base);
                self.foreign_value(&over.specifier, &over.decode);
                let encode = match &over.encode {
                    Some(encode) => {
                        self.foreign_value(&over.specifier, encode);
                        encode.clone()
                    }
                    None => "(value) => value".to_owned(),
                };
                self.hoist(
                    &over.name,
                    uf_infra::into_string(uf_infra::cstr!(
                        "{alias}.map({base_ref}, {}, {encode})",
                        over.decode
                    )),
                )
            }
        }
    }

    /// Record what the Flow type of `mapped` needs imported, and return it.
    fn type_of(&mut self, mapped: &Mapped) -> String {
        self.note_types(&mapped.codec);
        if mapped.flow.contains("JsonValue") {
            self.runtime_types.insert("JsonValue");
        }
        mapped.flow_type()
    }

    fn note_types(&mut self, codec: &Codec) {
        match codec {
            Codec::Enum { type_name, .. } if !self.is_models => {
                self.model_types.insert(type_name.clone());
            }
            Codec::Array(inner) => self.note_types(inner),
            Codec::Mapped { over, .. } => {
                let (specifier, name) = (over.specifier.clone(), over.name.clone());
                self.foreign_type(&specifier, &name);
            }
            _ => {}
        }
    }

    fn decode(&mut self, mapped: &Mapped, value: &str) -> String {
        let alias = self.alias;
        if mapped.unknown {
            self.uses_codecs = true;
            return uf_infra::into_string(uf_infra::cstr!("{alias}.unknown.decode({value})"));
        }
        let codec = self.codec(&mapped.codec);
        if mapped.nullable {
            uf_infra::into_string(uf_infra::cstr!("{alias}.nullable({codec}, {value})"))
        } else {
            uf_infra::into_string(uf_infra::cstr!("{codec}.decode({value})"))
        }
    }

    fn encode(&mut self, mapped: &Mapped, value: &str) -> String {
        let alias = self.alias;
        if mapped.unknown {
            self.uses_codecs = true;
            return uf_infra::into_string(uf_infra::cstr!("{alias}.unknown.encode({value})"));
        }
        let codec = self.codec(&mapped.codec);
        if mapped.nullable {
            uf_infra::into_string(uf_infra::cstr!("{alias}.param({codec}, {value})"))
        } else {
            uf_infra::into_string(uf_infra::cstr!("{codec}.encode({value})"))
        }
    }

    /// The fields of a query's result, and how many positions they take.
    fn row_fields(&mut self, query: &Query) -> Result<(Vec<RowField>, usize), String> {
        let shared = self.shared;
        let context = shared.context();
        let mut fields = Vec::new();
        let mut names = BTreeSet::new();
        let mut at = 0;
        for column in &query.columns {
            if let Some(embed) = &column.embed_table {
                let model = shared.model(embed).ok_or_else(|| {
                    uf_infra::into_string(uf_infra::cstr!(
                        "sqlc.embed({}) names no table in the schema",
                        embed.name
                    ))
                })?;
                let mut inner = Vec::new();
                let mut inner_names = BTreeSet::new();
                for table_column in &model.columns {
                    let mapped =
                        context.map(&with_table(table_column, &model.schema, &model.sql_name));
                    let name = unique(&mut inner_names, shared.field_name(&table_column.name));
                    let decode = self.decode(&mapped, uf_infra::cstr!("row[{at}]").as_str());
                    inner.push(uf_infra::into_string(uf_infra::cstr!(
                        "{}: {decode}",
                        key(&name)
                    )));
                    at += 1;
                }
                let name = unique(&mut names, shared.field_name(&embed.name));
                if !self.is_models {
                    self.model_types.insert(model.type_name.clone());
                }
                fields.push(RowField {
                    name,
                    flow: model.type_name.clone(),
                    decode: uf_infra::into_string(uf_infra::cstr!("{{ {} }}", inner.join(", "))),
                });
                continue;
            }
            let mapped = context.map(column);
            let flow = self.type_of(&mapped);
            let decode = self.decode(&mapped, uf_infra::cstr!("row[{at}]").as_str());
            at += 1;
            let name = unique(&mut names, shared.field_name(&column.name));
            fields.push(RowField { name, flow, decode });
        }
        Ok((fields, at))
    }

    /// The model a result is exactly a row of, if it is one.
    fn whole_model(&self, query: &Query) -> Option<&'a Model> {
        let shared: &'a Shared<'a> = self.shared;
        let first = query.columns.first()?;
        let table = first.table.as_ref()?;
        let model = shared.model(table)?;
        let same = model.columns.len() == query.columns.len()
            && query
                .columns
                .iter()
                .zip(&model.columns)
                .all(|(column, table_column)| {
                    column.embed_table.is_none()
                        && column
                            .table
                            .as_ref()
                            .is_some_and(|t| t.name == table.name && t.schema == table.schema)
                        && column.name == table_column.name
                        && column.original_name == table_column.name
                });
        same.then_some(model)
    }

    fn query(&mut self, query: &Query) -> Result<(), String> {
        let shared = self.shared;
        let context = shared.context();
        let mut function = camel(&query.name);
        if is_reserved(&function) {
            function.push_str("Query");
        }
        let prefix = pascal(&query.name);
        let cmd = query.cmd.as_str();
        let sql_name = uf_infra::into_string(uf_infra::cstr!(
            "{function}{}",
            if cmd == ":copyfrom" { "Plan" } else { "Sql" }
        ));
        self.claim(&function)?;
        self.claim(&sql_name)?;
        let rows = matches!(cmd, ":one" | ":many" | ":batchone" | ":batchmany");
        let is_batch = cmd.starts_with(":batch");
        let is_copy = cmd == ":copyfrom";
        if !matches!(
            cmd,
            ":one"
                | ":many"
                | ":exec"
                | ":execrows"
                | ":execresult"
                | ":execlastid"
                | ":copyfrom"
                | ":batchexec"
                | ":batchmany"
                | ":batchone"
        ) {
            return Err(uf_infra::into_string(uf_infra::cstr!(
                "the command `{cmd}` is not one sqlc defines"
            )));
        }

        // Arguments: one field per distinct name. A named parameter used
        // twice is one argument; two unnamed parameters that happen to share a
        // column name are two.
        let mut arg_fields: Vec<(String, String)> = Vec::new();
        let mut param_field: Vec<String> = Vec::new();
        let mut param_mapped: Vec<Mapped> = Vec::new();
        let mut names = BTreeSet::new();
        let mut named: BTreeMap<String, (String, Mapped)> = BTreeMap::new();
        for param in &query.params {
            let column = param.column.clone().unwrap_or_default();
            let sql_name = if column.name.is_empty() {
                uf_infra::into_string(uf_infra::cstr!("column_{}", param.number))
            } else {
                column.name.clone()
            };
            let mut mapped = context.map(&column);
            let slice = column.is_sqlc_slice && shared.engine != Engine::Postgresql;
            let field_name = if column.is_named_param
                && let Some((existing, first)) = named.get(&sql_name)
            {
                // One argument, so one type: the first place it appears decides
                // it, and every other place encodes it the same way.
                mapped = first.clone();
                existing.clone()
            } else {
                let name = unique(&mut names, shared.field_name(&sql_name));
                let mut flow = self.type_of(&mapped);
                if slice {
                    flow =
                        uf_infra::into_string(uf_infra::cstr!("$ReadOnlyArray<{}>", mapped.flow));
                }
                arg_fields.push((name.clone(), flow));
                if column.is_named_param {
                    named.insert(sql_name.clone(), (name.clone(), mapped.clone()));
                }
                name
            };
            if slice {
                // The elements of a slice are the column's type, never null.
                mapped.nullable = false;
            }
            param_field.push(field_name);
            param_mapped.push(mapped);
        }
        let args_type = uf_infra::into_string(uf_infra::cstr!("{prefix}Args"));
        if !arg_fields.is_empty() || is_batch || is_copy {
            self.claim(&args_type)?;
            let fields: String = arg_fields
                .iter()
                .map(|(name, flow)| {
                    uf_infra::into_string(uf_infra::cstr!("  readonly {}: {flow},\n", key(name)))
                })
                .collect();
            self.body.push_str(&uf_infra::into_string(uf_infra::cstr!(
                "export type {args_type} = {{|\n{fields}|}};\n\n"
            )));
        }

        let is_slice = |index: usize| {
            query.params[index]
                .column
                .as_ref()
                .is_some_and(|column| column.is_sqlc_slice)
                && shared.engine != Engine::Postgresql
        };
        let encode_param = |module: &mut Self, index: usize| -> String {
            let value = uf_infra::into_string(uf_infra::cstr!("args.{}", param_field[index]));
            if is_slice(index) {
                module.uses_sql = true;
                let codec = if param_mapped[index].unknown {
                    uf_infra::into_string(uf_infra::cstr!("{}.unknown", module.alias))
                } else {
                    module.codec(&param_mapped[index].codec)
                };
                uf_infra::into_string(uf_infra::cstr!("sql.slice({value}.map({codec}.encode))"))
            } else {
                module.encode(&param_mapped[index], &value)
            }
        };

        // The text, and the values in the order the text binds them.
        let dialect = match shared.engine {
            Engine::Postgresql => Dialect::Postgresql,
            Engine::Mysql => Dialect::Mysql,
            Engine::Sqlite => Dialect::Sqlite,
        };
        let mut text_const = String::new();
        let mut values: Vec<String> = Vec::new();
        let mut expands = false;
        if is_copy {
            let split = placeholders::copy_split(&query.text, dialect, &query.params)?;
            let tuple: Vec<String> = split.tuple.iter().map(|part| js_string(part)).collect();
            let refs: Vec<String> = split.refs.iter().map(ToString::to_string).collect();
            self.runtime_types.insert("CopyPlan");
            text_const = uf_infra::into_string(uf_infra::cstr!(
                "const {sql_name}: CopyPlan = {{\n  head: {},\n  tuple: [{}],\n  refs: [{}],\n  tail: {},\n}};\n\n",
                js_string(&split.head),
                tuple.join(", "),
                refs.join(", "),
                js_string(&split.tail),
            ));
            values = (0..query.params.len())
                .map(|index| encode_param(self, index))
                .collect();
        } else if shared.engine == Engine::Postgresql {
            let found = placeholders::scan(&query.text, dialect);
            placeholders::bind_order(&found, &query.params)?;
            let highest = query
                .params
                .iter()
                .map(|param| param.number)
                .max()
                .unwrap_or(0);
            for number in 1..=highest {
                match query.params.iter().position(|param| param.number == number) {
                    Some(index) => values.push(encode_param(self, index)),
                    None => values.push("null".to_owned()),
                }
            }
            text_const = uf_infra::into_string(uf_infra::cstr!(
                "const {sql_name} = {};\n\n",
                js_string(&query.text)
            ));
        } else {
            let found = placeholders::scan(&query.text, dialect);
            let order = placeholders::bind_order(&found, &query.params)?;
            let parts = placeholders::positional_parts(&query.text, &found);
            expands = parts.len() > 1;
            if expands {
                let parts: Vec<String> = parts.iter().map(|part| js_string(part)).collect();
                text_const = uf_infra::into_string(uf_infra::cstr!(
                    "const {sql_name} = [{}];\n\n",
                    parts.join(", ")
                ));
            } else {
                uf_infra::append!(
                    text_const,
                    "const {sql_name} = {};\n\n",
                    js_string(&parts[0])
                );
            }
            for index in order {
                values.push(encode_param(self, index));
            }
        }
        let values_list = uf_infra::into_string(uf_infra::cstr!("[{}]", values.join(", ")));

        // The row.
        let mut result_type = String::new();
        let mut decoder = String::new();
        let mut width = 0;
        if rows {
            let (fields, positions) = self.row_fields(query)?;
            width = positions;
            let single = fields.len() == 1
                && query.columns.len() == 1
                && query.columns[0].embed_table.is_none();
            let single_nullable = single && {
                let mapped = context.map(&query.columns[0]);
                mapped.nullable || mapped.unknown
            };
            if single && (!single_nullable || matches!(cmd, ":many" | ":batchmany")) {
                result_type = fields[0].flow.clone();
                decoder = uf_infra::into_string(uf_infra::cstr!("(row) => {}", fields[0].decode));
            } else {
                if let Some(model) = self.whole_model(query) {
                    result_type = model.type_name.clone();
                    self.model_types.insert(model.type_name.clone());
                } else {
                    result_type = uf_infra::into_string(uf_infra::cstr!("{prefix}Row"));
                    self.claim(&result_type)?;
                    let lines: String = fields
                        .iter()
                        .map(|field| {
                            uf_infra::into_string(uf_infra::cstr!(
                                "  readonly {}: {},\n",
                                key(&field.name),
                                field.flow
                            ))
                        })
                        .collect();
                    self.body.push_str(&uf_infra::into_string(uf_infra::cstr!(
                        "export type {result_type} = {{|\n{lines}|}};\n\n"
                    )));
                }
                let entries: Vec<String> = fields
                    .iter()
                    .map(|field| {
                        uf_infra::into_string(uf_infra::cstr!(
                            "{}: {}",
                            key(&field.name),
                            field.decode
                        ))
                    })
                    .collect();
                decoder = uf_infra::into_string(uf_infra::cstr!(
                    "(row) => ({{ {} }})",
                    entries.join(", ")
                ));
            }
        }

        self.uses_sql = true;
        self.runtime_types.insert("Queryable");
        let name = js_string(&query.name);
        let has_args = !arg_fields.is_empty();
        let (text_expr, params_expr, prelude) = if expands {
            (
                "text".to_owned(),
                "params".to_owned(),
                uf_infra::into_string(uf_infra::cstr!(
                    "const {{ text, params }} = sql.expand({sql_name}, {values_list});\n"
                )),
            )
        } else {
            (sql_name.clone(), values_list.clone(), String::new())
        };
        let call = |db: &str| -> String {
            match cmd {
                ":one" | ":batchone" => uf_infra::into_string(uf_infra::cstr!(
                    "sql.one({db}, {name}, {text_expr}, {params_expr}, {width}, {decoder})"
                )),
                ":many" | ":batchmany" => uf_infra::into_string(uf_infra::cstr!(
                    "sql.many({db}, {name}, {text_expr}, {params_expr}, {width}, {decoder})"
                )),
                ":execrows" => uf_infra::into_string(uf_infra::cstr!(
                    "sql.execRows({db}, {name}, {text_expr}, {params_expr})"
                )),
                ":execresult" => uf_infra::into_string(uf_infra::cstr!(
                    "sql.execResult({db}, {name}, {text_expr}, {params_expr})"
                )),
                ":execlastid" => uf_infra::into_string(uf_infra::cstr!(
                    "sql.execLastId({db}, {name}, {text_expr}, {params_expr})"
                )),
                _ => uf_infra::into_string(uf_infra::cstr!(
                    "sql.exec({db}, {name}, {text_expr}, {params_expr})"
                )),
            }
        };
        let item_type = match cmd {
            ":one" | ":batchone" => uf_infra::into_string(uf_infra::cstr!("{result_type} | null")),
            ":many" | ":batchmany" => {
                uf_infra::into_string(uf_infra::cstr!("Array<{result_type}>"))
            }
            ":execrows" | ":copyfrom" => "number".to_owned(),
            ":execresult" => {
                self.runtime_types.insert("ExecResult");
                "ExecResult".to_owned()
            }
            ":execlastid" => "bigint".to_owned(),
            _ => "void".to_owned(),
        };
        let body = if is_copy {
            uf_infra::into_string(uf_infra::cstr!(
                "  return sql.copyFrom(db, {name}, {sql_name}, rows.map((args) => {values_list}));\n"
            ))
        } else if is_batch {
            let inner = if prelude.is_empty() {
                call("q")
            } else {
                uf_infra::into_string(uf_infra::cstr!(
                    "{{\n    {prelude}    return {};\n  }}",
                    call("q")
                ))
            };
            let then = if cmd == ":batchexec" {
                ".then(() => {})"
            } else {
                ""
            };
            uf_infra::into_string(uf_infra::cstr!(
                "  return sql.batch(db, items, (q, args) => {inner}){then};\n"
            ))
        } else {
            let prelude = if prelude.is_empty() {
                String::new()
            } else {
                uf_infra::into_string(uf_infra::cstr!("  {prelude}"))
            };
            uf_infra::into_string(uf_infra::cstr!("{prelude}  return {};\n", call("db")))
        };
        let (signature, returns) = if is_copy {
            (
                uf_infra::into_string(uf_infra::cstr!(
                    "db: Queryable, rows: $ReadOnlyArray<{args_type}>"
                )),
                item_type,
            )
        } else if is_batch {
            let returns = if cmd == ":batchexec" {
                "void".to_owned()
            } else {
                uf_infra::into_string(uf_infra::cstr!("Array<{item_type}>"))
            };
            (
                uf_infra::into_string(uf_infra::cstr!(
                    "db: Queryable, items: $ReadOnlyArray<{args_type}>"
                )),
                returns,
            )
        } else if has_args {
            (
                uf_infra::into_string(uf_infra::cstr!("db: Queryable, args: {args_type}")),
                item_type,
            )
        } else {
            ("db: Queryable".to_owned(), item_type)
        };
        self.body.push_str(&text_const);
        self.body.push_str(&doc(&query.comments, ""));
        uf_infra::append!(
            self.body,
            "export function {function}({signature}): Promise<{returns}> {{\n{body}}}\n\n"
        );
        Ok(())
    }

    /// Imports, hoisted codecs, the body and the header, unsigned.
    ///
    /// An import is written only if its name appears in what follows it: a
    /// row type that turned out to be a model's does not need the types its
    /// columns were checked against.
    fn finish(mut self, what: &str) -> String {
        let shared = self.shared;
        let mut rest = String::new();
        for (name, expression) in &self.hoisted {
            uf_infra::append!(rest, "const {name} = {expression};\n");
        }
        rest.push_str(&self.body);
        let used = identifiers(&rest);
        self.runtime_types.retain(|name| used.contains(*name));
        self.model_types.retain(|name| used.contains(name));
        self.model_values.retain(|name| used.contains(name));
        for (types, values) in self.foreign.values_mut() {
            types.retain(|name| used.contains(name));
            values.retain(|name| used.contains(name));
        }
        self.uses_sql &= used.contains("sql");
        self.uses_codecs &= used.contains(self.alias);
        let runtime = shared.options.runtime();
        let mut out = header(shared, what);
        out.push('\n');
        if !self.runtime_types.is_empty() {
            let names: Vec<&str> = self.runtime_types.iter().copied().collect();
            uf_infra::append!(
                out,
                "import type {{ {} }} from {};\n",
                names.join(", "),
                js_string(runtime)
            );
        }
        if self.uses_sql {
            uf_infra::append!(out, "import * as sql from {};\n", js_string(runtime));
        }
        if self.uses_codecs {
            uf_infra::append!(
                out,
                "import * as {} from {};\n",
                self.alias,
                js_string(&uf_infra::into_string(uf_infra::cstr!(
                    "{runtime}/{}",
                    shared.engine.module()
                )))
            );
        }
        for (specifier, (types, values)) in &self.foreign {
            if !types.is_empty() {
                let names: Vec<&str> = types.iter().map(String::as_str).collect();
                uf_infra::append!(
                    out,
                    "import type {{ {} }} from {};\n",
                    names.join(", "),
                    js_string(specifier)
                );
            }
            if !values.is_empty() {
                let names: Vec<&str> = values.iter().map(String::as_str).collect();
                uf_infra::append!(
                    out,
                    "import {{ {} }} from {};\n",
                    names.join(", "),
                    js_string(specifier)
                );
            }
        }
        if !self.model_types.is_empty() {
            let names: Vec<&str> = self.model_types.iter().map(String::as_str).collect();
            uf_infra::append!(
                out,
                "import type {{ {} }} from \"./models.js\";\n",
                names.join(", ")
            );
        }
        if !self.model_values.is_empty() {
            let names: Vec<&str> = self.model_values.iter().map(String::as_str).collect();
            uf_infra::append!(
                out,
                "import {{ {} }} from \"./models.js\";\n",
                names.join(", ")
            );
        }
        out.push('\n');
        for (name, expression) in &self.hoisted {
            uf_infra::append!(out, "const {name} = {expression};\n");
        }
        if !self.hoisted.is_empty() {
            out.push('\n');
        }
        out.push_str(self.body.trim_end());
        out.push('\n');
        out
    }
}
