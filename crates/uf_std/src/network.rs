//! HTTP, WebSocket, SQL and DNS surface descriptions.
//!
//! These are contracts rather than clients: the shapes a route, a socket, a
//! driver or a lookup is described with, so the native bindings and the Flow
//! declarations cannot drift apart.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};

/// HTTP method contract for `@uniflowed/std/http`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// GET.
    Get,
    /// POST.
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
    /// DELETE.
    Delete,
}

/// HTTP route descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRoute {
    /// Method matched by the route.
    pub method: HttpMethod,
    /// Route path.
    pub path: CompactString,
}

/// WebSocket mode used by `@uniflowed/std/ws`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSocketMode {
    /// WebSocket interface.
    WebSocket,
    /// Stream-oriented WebSocket contract.
    WebSocketStream,
}

/// SQL driver kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SqlDriverKind {
    /// SQLite driver.
    Sqlite,
    /// Postgres driver.
    Postgres,
    /// MySQL driver.
    Mysql,
}

/// SQL driver descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlDriver {
    /// Driver kind.
    pub kind: SqlDriverKind,
    /// Whether statements are prepared by default.
    pub prepared_by_default: bool,
}

/// DNS record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    /// IPv4 address record.
    A,
    /// IPv6 address record.
    Aaaa,
    /// Canonical name record.
    Cname,
    /// Mail exchange record.
    Mx,
    /// Text record.
    Txt,
}

/// DNS query descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsQuery {
    /// Record name.
    pub name: CompactString,
    /// Record type.
    pub record_type: DnsRecordType,
}

impl DnsQuery {
    /// Create a DNS query descriptor.
    pub fn new(name: &str, record_type: DnsRecordType) -> Self {
        Self {
            name: name.to_compact_string(),
            record_type,
        }
    }
}
