//! sqlc's plugin protocol: `plugin.GenerateRequest` in, `GenerateResponse` out.
//!
//! The messages mirror `protos/plugin/codegen.proto` in sqlc (fields this
//! generator does not read are skipped, not rejected, so a newer sqlc that
//! adds one still works). Two encodings arrive:
//!
//! - **protobuf**, the default for process plugins and the only one for WASM
//!   plugins. Decoded by hand below: the protocol needs varints, length-
//!   delimited fields and nothing else, and a hand-written reader keeps the
//!   crate free of `protoc` and `prost` so it builds for `wasm32-wasip1` as is.
//! - **JSON**, when a process plugin is configured with `format: json`
//!   (protojson with proto field names). The fixtures under `tests/sqlc` are
//!   stored this way because a reviewer can read them.

use serde::{Deserialize, Deserializer};

/// Why a request could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not read sqlc's request: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

/// `plugin.GenerateRequest`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct GenerateRequest {
    pub settings: Option<Settings>,
    pub catalog: Option<Catalog>,
    #[serde(deserialize_with = "null_as_default")]
    pub queries: Vec<Query>,
    pub sqlc_version: String,
    #[serde(deserialize_with = "base64_bytes")]
    pub plugin_options: Vec<u8>,
    #[serde(deserialize_with = "base64_bytes")]
    pub global_options: Vec<u8>,
}

/// `plugin.Settings`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: String,
    pub engine: String,
    #[serde(deserialize_with = "null_as_default")]
    pub schema: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub queries: Vec<String>,
    pub codegen: Option<Codegen>,
}

/// `plugin.Codegen`: the `codegen` entry of `sqlc.yaml` that asked for this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Codegen {
    pub out: String,
    pub plugin: String,
    #[serde(deserialize_with = "base64_bytes")]
    pub options: Vec<u8>,
}

/// `plugin.Catalog`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub comment: String,
    pub default_schema: String,
    pub name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub schemas: Vec<Schema>,
}

/// `plugin.Schema`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Schema {
    pub comment: String,
    pub name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub tables: Vec<Table>,
    #[serde(deserialize_with = "null_as_default")]
    pub enums: Vec<Enum>,
}

/// `plugin.Enum`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Enum {
    pub name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub vals: Vec<String>,
    pub comment: String,
}

/// `plugin.Table`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Table {
    pub rel: Option<Identifier>,
    #[serde(deserialize_with = "null_as_default")]
    pub columns: Vec<Column>,
    pub comment: String,
}

/// `plugin.Identifier`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Identifier {
    pub catalog: String,
    pub schema: String,
    pub name: String,
}

/// `plugin.Column`: a table's column, a query's result column, or a parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Column {
    pub name: String,
    pub not_null: bool,
    pub is_array: bool,
    pub comment: String,
    pub length: i32,
    pub is_named_param: bool,
    pub is_func_call: bool,
    pub scope: String,
    pub table: Option<Identifier>,
    pub table_alias: String,
    #[serde(rename = "type")]
    pub type_: Option<Identifier>,
    pub is_sqlc_slice: bool,
    pub embed_table: Option<Identifier>,
    pub original_name: String,
    pub unsigned: bool,
    pub array_dims: i32,
}

/// `plugin.Query`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Query {
    pub text: String,
    pub name: String,
    pub cmd: String,
    #[serde(deserialize_with = "null_as_default")]
    pub columns: Vec<Column>,
    #[serde(deserialize_with = "null_as_default")]
    pub params: Vec<Parameter>,
    #[serde(deserialize_with = "null_as_default")]
    pub comments: Vec<String>,
    pub filename: String,
    pub insert_into_table: Option<Identifier>,
}

/// `plugin.Parameter`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Parameter {
    pub number: i32,
    pub column: Option<Column>,
}

/// One file of `plugin.GenerateResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub contents: String,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn base64_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine as _;
    let text = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
    base64::engine::general_purpose::STANDARD
        .decode(text.as_bytes())
        .map_err(serde::de::Error::custom)
}

/// Read a request sqlc sent as JSON (`format: json`).
///
/// # Errors
///
/// When the bytes are not a JSON `GenerateRequest`.
pub fn request_from_json(bytes: &[u8]) -> Result<GenerateRequest, DecodeError> {
    let mut request: GenerateRequest =
        serde_json::from_slice(bytes).map_err(|error| DecodeError(error.to_string()))?;
    normalize(&mut request);
    Ok(request)
}

/// An identifier with every part empty means "none" — sqlc sends one for a
/// result column that comes from no table — and the two encodings disagree on
/// whether it is present at all: protobuf keeps the empty message, JSON with
/// defaults left out drops it. Both become `None`.
fn normalize(request: &mut GenerateRequest) {
    fn identifier(slot: &mut Option<Identifier>) {
        if slot
            .as_ref()
            .is_some_and(|id| id.catalog.is_empty() && id.schema.is_empty() && id.name.is_empty())
        {
            *slot = None;
        }
    }
    fn column(column: &mut Column) {
        identifier(&mut column.table);
        identifier(&mut column.type_);
        identifier(&mut column.embed_table);
    }
    if let Some(catalog) = &mut request.catalog {
        for schema in &mut catalog.schemas {
            for table in &mut schema.tables {
                identifier(&mut table.rel);
                table.columns.iter_mut().for_each(column);
            }
        }
    }
    for query in &mut request.queries {
        identifier(&mut query.insert_into_table);
        query.columns.iter_mut().for_each(column);
        for param in &mut query.params {
            if let Some(param_column) = &mut param.column {
                column(param_column);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Protobuf.

/// A cursor over one message's bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// A field's value, by wire type.
enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    /// Fixed 32- and 64-bit values: no field sqlc sends has one, so they are
    /// read only to be skipped.
    Fixed,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn varint(&mut self) -> Result<u64, DecodeError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let Some(&byte) = self.bytes.get(self.at) else {
                return Err(DecodeError(
                    "a varint runs past the end of the message".into(),
                ));
            };
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DecodeError("a varint is longer than ten bytes".into()))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| DecodeError("a field runs past the end of the message".into()))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    /// The next field, or `None` at the end of the message.
    fn field(&mut self) -> Result<Option<(u64, Value<'a>)>, DecodeError> {
        if self.at >= self.bytes.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = key >> 3;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.take(8)?;
                Value::Fixed
            }
            2 => {
                let len = usize::try_from(self.varint()?)
                    .map_err(|_| DecodeError("a field is longer than memory".into()))?;
                Value::Bytes(self.take(len)?)
            }
            5 => {
                self.take(4)?;
                Value::Fixed
            }
            wire => {
                return Err(DecodeError(uf_infra::into_string(
                    compact_str::format_compact!(
                        "field {number} has wire type {wire}, which proto3 does not use"
                    ),
                )));
            }
        };
        Ok(Some((number, value)))
    }
}

fn string(value: &Value<'_>) -> Result<String, DecodeError> {
    match value {
        Value::Bytes(bytes) => String::from_utf8(bytes.to_vec())
            .map_err(|_| DecodeError("a string field is not UTF-8".into())),
        _ => Err(DecodeError("a string field has the wrong wire type".into())),
    }
}

fn bytes(value: &Value<'_>) -> Result<Vec<u8>, DecodeError> {
    match value {
        Value::Bytes(bytes) => Ok(bytes.to_vec()),
        _ => Err(DecodeError("a bytes field has the wrong wire type".into())),
    }
}

fn boolean(value: &Value<'_>) -> Result<bool, DecodeError> {
    match value {
        Value::Varint(value) => Ok(*value != 0),
        _ => Err(DecodeError("a bool field has the wrong wire type".into())),
    }
}

fn int32(value: &Value<'_>) -> Result<i32, DecodeError> {
    match value {
        // A negative int32 is sign-extended to ten bytes on the wire; the low
        // 32 bits are the value.
        Value::Varint(value) => Ok(*value as i32),
        _ => Err(DecodeError("an int32 field has the wrong wire type".into())),
    }
}

fn message<T>(
    value: &Value<'_>,
    decode: fn(&[u8]) -> Result<T, DecodeError>,
) -> Result<T, DecodeError> {
    match value {
        Value::Bytes(bytes) => decode(bytes),
        _ => Err(DecodeError(
            "a message field has the wrong wire type".into(),
        )),
    }
}

/// Read a request sqlc sent as protobuf.
///
/// # Errors
///
/// When the bytes are not a well-formed `GenerateRequest`.
pub fn request_from_protobuf(bytes: &[u8]) -> Result<GenerateRequest, DecodeError> {
    let mut out = GenerateRequest::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.settings = Some(message(&value, settings)?),
            2 => out.catalog = Some(message(&value, catalog)?),
            3 => out.queries.push(message(&value, query)?),
            4 => out.sqlc_version = string(&value)?,
            5 => out.plugin_options = self::bytes(&value)?,
            6 => out.global_options = self::bytes(&value)?,
            _ => {}
        }
    }
    normalize(&mut out);
    Ok(out)
}

fn settings(bytes: &[u8]) -> Result<Settings, DecodeError> {
    let mut out = Settings::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.version = string(&value)?,
            2 => out.engine = string(&value)?,
            3 => out.schema.push(string(&value)?),
            4 => out.queries.push(string(&value)?),
            12 => out.codegen = Some(message(&value, codegen)?),
            _ => {}
        }
    }
    Ok(out)
}

fn codegen(bytes: &[u8]) -> Result<Codegen, DecodeError> {
    let mut out = Codegen::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.out = string(&value)?,
            2 => out.plugin = string(&value)?,
            3 => out.options = self::bytes(&value)?,
            _ => {}
        }
    }
    Ok(out)
}

fn catalog(bytes: &[u8]) -> Result<Catalog, DecodeError> {
    let mut out = Catalog::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.comment = string(&value)?,
            2 => out.default_schema = string(&value)?,
            3 => out.name = string(&value)?,
            4 => out.schemas.push(message(&value, schema)?),
            _ => {}
        }
    }
    Ok(out)
}

fn schema(bytes: &[u8]) -> Result<Schema, DecodeError> {
    let mut out = Schema::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.comment = string(&value)?,
            2 => out.name = string(&value)?,
            3 => out.tables.push(message(&value, table)?),
            4 => out.enums.push(message(&value, enumeration)?),
            _ => {}
        }
    }
    Ok(out)
}

fn enumeration(bytes: &[u8]) -> Result<Enum, DecodeError> {
    let mut out = Enum::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.name = string(&value)?,
            2 => out.vals.push(string(&value)?),
            3 => out.comment = string(&value)?,
            _ => {}
        }
    }
    Ok(out)
}

fn table(bytes: &[u8]) -> Result<Table, DecodeError> {
    let mut out = Table::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.rel = Some(message(&value, identifier)?),
            2 => out.columns.push(message(&value, column)?),
            3 => out.comment = string(&value)?,
            _ => {}
        }
    }
    Ok(out)
}

fn identifier(bytes: &[u8]) -> Result<Identifier, DecodeError> {
    let mut out = Identifier::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.catalog = string(&value)?,
            2 => out.schema = string(&value)?,
            3 => out.name = string(&value)?,
            _ => {}
        }
    }
    Ok(out)
}

fn column(bytes: &[u8]) -> Result<Column, DecodeError> {
    let mut out = Column::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.name = string(&value)?,
            3 => out.not_null = boolean(&value)?,
            4 => out.is_array = boolean(&value)?,
            5 => out.comment = string(&value)?,
            6 => out.length = int32(&value)?,
            7 => out.is_named_param = boolean(&value)?,
            8 => out.is_func_call = boolean(&value)?,
            9 => out.scope = string(&value)?,
            10 => out.table = Some(message(&value, identifier)?),
            11 => out.table_alias = string(&value)?,
            12 => out.type_ = Some(message(&value, identifier)?),
            13 => out.is_sqlc_slice = boolean(&value)?,
            14 => out.embed_table = Some(message(&value, identifier)?),
            15 => out.original_name = string(&value)?,
            16 => out.unsigned = boolean(&value)?,
            17 => out.array_dims = int32(&value)?,
            _ => {}
        }
    }
    Ok(out)
}

fn query(bytes: &[u8]) -> Result<Query, DecodeError> {
    let mut out = Query::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.text = string(&value)?,
            2 => out.name = string(&value)?,
            3 => out.cmd = string(&value)?,
            4 => out.columns.push(message(&value, column)?),
            5 => out.params.push(message(&value, parameter)?),
            6 => out.comments.push(string(&value)?),
            7 => out.filename = string(&value)?,
            8 => out.insert_into_table = Some(message(&value, identifier)?),
            _ => {}
        }
    }
    Ok(out)
}

fn parameter(bytes: &[u8]) -> Result<Parameter, DecodeError> {
    let mut out = Parameter::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match number {
            1 => out.number = int32(&value)?,
            2 => out.column = Some(message(&value, column)?),
            _ => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The response.

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_bytes(out: &mut Vec<u8>, number: u64, bytes: &[u8]) {
    put_varint(out, (number << 3) | 2);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

/// Encode a `GenerateResponse` as protobuf.
#[must_use]
pub fn response_to_protobuf(files: &[File]) -> Vec<u8> {
    let mut out = Vec::new();
    for file in files {
        let mut message = Vec::with_capacity(file.name.len() + file.contents.len() + 8);
        put_bytes(&mut message, 1, file.name.as_bytes());
        put_bytes(&mut message, 2, file.contents.as_bytes());
        put_bytes(&mut out, 1, &message);
    }
    out
}

/// Encode a `GenerateResponse` as protojson, for `format: json`.
#[must_use]
pub fn response_to_json(files: &[File]) -> Vec<u8> {
    use base64::Engine as _;
    let files: Vec<serde_json::Value> = files
        .iter()
        .map(|file| {
            serde_json::json!({
                "name": file.name,
                "contents": base64::engine::general_purpose::STANDARD.encode(file.contents.as_bytes()),
            })
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({ "files": files })).unwrap_or_default()
}
