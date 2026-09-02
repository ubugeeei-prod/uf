//! The HTTP surface: parse a request head, decide, write a response.
//!
//! # Threat model
//!
//! [CVE-2025-29927] was an inbound header (`x-middleware-subrequest`) that
//! selected a dispatch path, so sending it skipped the middleware that did the
//! authorization. The lesson is not "validate that header better"; it is that
//! **no inbound header may participate in routing, dispatch, or authorization.**
//!
//! [`RequestHead`] enforces that structurally. Parsing keeps the method, the
//! request target, and exactly two header values — `Host` and `Origin` — and
//! drops every other header as it scans past it. There is no header map to
//! consult, so no future handler can grow a dependency on one. The two headers
//! that are kept can only ever turn a served request into a refusal
//! ([`crate::network`]); neither can select a root, a loader, a handler, or a
//! path.
//!
//! Everything else here is bounded on purpose: the request head has a byte
//! ceiling, the method is a closed enum, and the response body is a `Vec<u8>`
//! that came from a [`ResolvedFile`] whose size was already checked.
//!
//! [CVE-2025-29927]: https://nvd.nist.gov/vuln/detail/CVE-2025-29927

use std::fmt;

use thiserror::Error;

use crate::network::{NetworkDenial, NetworkPolicy};
use crate::policy::{FsPolicy, PolicyDenial};
use crate::resolve::{AccessDenied, resolve_with_policy};
use crate::target::RequestTarget;

#[cfg(test)]
mod tests;

/// Largest request head this server will buffer, in bytes.
pub const MAX_REQUEST_HEAD_BYTES: usize = 8 * 1024;

/// Most header lines this server will scan past.
pub const MAX_HEADER_LINES: usize = 128;

/// The health endpoint, matched as an exact request target.
pub const HEALTH_TARGET: &str = "/__uf/health";

/// The methods this server implements.
///
/// A closed enum: an unknown method becomes a `405`, never a string that some
/// later `match` might treat as familiar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`.
    Get,
    /// `HEAD`.
    Head,
    /// `OPTIONS`, including CORS preflights.
    Options,
}

impl Method {
    /// Parse a method token.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "GET" => Some(Self::Get),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }

    /// Whether this is a simple, side-effect-free request in the CORS sense.
    pub fn is_simple(self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }

    /// Whether a response to this method carries a body.
    pub fn wants_body(self) -> bool {
        matches!(self, Self::Get)
    }
}

/// Why a request head could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HttpError {
    /// The head exceeded [`MAX_REQUEST_HEAD_BYTES`].
    #[error("request head is larger than {MAX_REQUEST_HEAD_BYTES} bytes")]
    HeadTooLarge,
    /// The head is not valid UTF-8.
    #[error("request head is not valid UTF-8")]
    NonUtf8,
    /// The request line is not `METHOD SP TARGET SP VERSION`.
    #[error("malformed request line")]
    MalformedRequestLine,
    /// The method is not one this server implements.
    #[error("unsupported method")]
    UnsupportedMethod,
    /// The version token is not `HTTP/1.0` or `HTTP/1.1`.
    #[error("unsupported HTTP version")]
    UnsupportedVersion,
    /// More than [`MAX_HEADER_LINES`] header lines arrived.
    #[error("request carries more than {MAX_HEADER_LINES} header lines")]
    TooManyHeaders,
    /// A header line has no `:`.
    #[error("malformed header line")]
    MalformedHeader,
    /// `Host` or `Origin` appeared twice, which is request smuggling bait.
    #[error("duplicate {name} header")]
    DuplicateHeader {
        /// The header that repeated.
        name: &'static str,
    },
}

/// A parsed request head, reduced to the only four things that may matter.
///
/// Note what this struct does *not* have: a header map. Dropping every other
/// header during the scan is what makes "no inbound header influences routing"
/// a property of the type rather than a convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestHead<'a> {
    method: Method,
    target: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
}

impl<'a> RequestHead<'a> {
    /// Parse the bytes up to and including the blank line that ends the head.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError`] for a head that is too large, not UTF-8, or not
    /// shaped like an HTTP/1.x request head.
    pub fn parse(raw: &'a [u8]) -> Result<Self, HttpError> {
        if raw.len() > MAX_REQUEST_HEAD_BYTES {
            return Err(HttpError::HeadTooLarge);
        }
        let text = std::str::from_utf8(raw).map_err(|_| HttpError::NonUtf8)?;
        let mut lines = text.split("\r\n");
        let request_line = lines.next().ok_or(HttpError::MalformedRequestLine)?;

        let mut parts = request_line.split(' ');
        let method = parts.next().ok_or(HttpError::MalformedRequestLine)?;
        let target = parts.next().ok_or(HttpError::MalformedRequestLine)?;
        let version = parts.next().ok_or(HttpError::MalformedRequestLine)?;
        if parts.next().is_some() || target.is_empty() {
            return Err(HttpError::MalformedRequestLine);
        }
        if version != "HTTP/1.1" && version != "HTTP/1.0" {
            return Err(HttpError::UnsupportedVersion);
        }
        let method = Method::parse(method).ok_or(HttpError::UnsupportedMethod)?;

        let mut host = None;
        let mut origin = None;
        let mut seen = 0usize;
        for line in lines {
            if line.is_empty() {
                break;
            }
            seen += 1;
            if seen > MAX_HEADER_LINES {
                return Err(HttpError::TooManyHeaders);
            }
            let (name, value) = line.split_once(':').ok_or(HttpError::MalformedHeader)?;
            let value = value.trim();
            // Everything not named here is read past and forgotten. That is the
            // guard, not an optimization.
            if name.eq_ignore_ascii_case("host") {
                if host.replace(value).is_some() {
                    return Err(HttpError::DuplicateHeader { name: "Host" });
                }
            } else if name.eq_ignore_ascii_case("origin") && origin.replace(value).is_some() {
                return Err(HttpError::DuplicateHeader { name: "Origin" });
            }
        }

        Ok(Self {
            method,
            target,
            host,
            origin,
        })
    }

    /// The request method.
    pub fn method(&self) -> Method {
        self.method
    }

    /// The raw, still-unvalidated request target.
    pub fn target(&self) -> &'a str {
        self.target
    }

    /// The `Host` header value, if one arrived.
    pub fn host(&self) -> Option<&'a str> {
        self.host
    }

    /// The `Origin` header value, if one arrived.
    pub fn origin(&self) -> Option<&'a str> {
        self.origin
    }
}

/// The status codes this server emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// The request was served.
    Ok,
    /// The request was not a request this server can parse.
    BadRequest,
    /// The request was understood and refused.
    Forbidden,
    /// Nothing is served at that path.
    NotFound,
    /// The method is not implemented.
    MethodNotAllowed,
    /// The file is over the size ceiling.
    PayloadTooLarge,
    /// The filesystem failed in a way that is not the client's business.
    InternalServerError,
}

impl Status {
    /// The numeric code.
    pub const fn code(self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::BadRequest => 400,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::PayloadTooLarge => 413,
            Self::InternalServerError => 500,
        }
    }

    /// The reason phrase.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::BadRequest => "Bad Request",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::PayloadTooLarge => "Payload Too Large",
            Self::InternalServerError => "Internal Server Error",
        }
    }

    /// Whether the request was served.
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Ok)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.code(), self.reason())
    }
}

/// A response, built entirely from server-controlled values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status to send.
    pub status: Status,
    /// The `Content-Type` header value.
    pub content_type: &'static str,
    /// The `x-uf-loader` header value: which closed-enum loader was selected.
    ///
    /// A response header only. Nothing in this crate reads a loader from an
    /// inbound header.
    pub loader: &'static str,
    /// The body, empty for `HEAD` and for refusals.
    pub body: Vec<u8>,
}

impl Response {
    /// A refusal carrying only its status.
    pub fn refusal(status: Status) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            loader: "none",
            body: Vec::new(),
        }
    }

    /// Serialize the response as HTTP/1.1 bytes.
    ///
    /// Nothing from the request reaches the head: no reflected path, no
    /// reflected origin, no reflected header. `nosniff` is unconditional so a
    /// project file can never be re-interpreted as HTML by the browser.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.body.len() + 256);
        out.extend_from_slice(
            format!(
                "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nx-uf-loader: {}\r\nx-content-type-options: nosniff\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
                self.status.code(),
                self.status.reason(),
                self.content_type,
                self.body.len(),
                self.loader,
            )
            .as_bytes(),
        );
        out.extend_from_slice(&self.body);
        out
    }
}

/// Map a refusal onto the status the client sees.
///
/// A denied path and a missing path both answer with as little as possible:
/// [`AccessDenied::Denied`] is a flat `403` and never says which pattern
/// matched, and a resolution failure is a flat `404`.
pub fn status_for(denied: &AccessDenied) -> Status {
    match denied {
        AccessDenied::InvalidTarget(_)
        | AccessDenied::InvalidPercentEncoding { .. }
        | AccessDenied::DoubleEncoded
        | AccessDenied::NonUtf8
        | AccessDenied::ForbiddenByte { .. }
        | AccessDenied::TooDeep => Status::BadRequest,
        AccessDenied::Escape
        | AccessDenied::FilesystemPrefix
        | AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
        | AccessDenied::Denied(PolicyDenial::OutsideAllowedRoots { .. }) => Status::Forbidden,
        AccessDenied::NotFound | AccessDenied::NotARegularFile { .. } => Status::NotFound,
        AccessDenied::TooLarge { .. } => Status::PayloadTooLarge,
        AccessDenied::Io { .. } => Status::InternalServerError,
    }
}

/// Map a network refusal onto a status.
pub fn status_for_network(denial: &NetworkDenial) -> Status {
    match denial {
        NetworkDenial::MissingHost | NetworkDenial::MalformedHost => Status::BadRequest,
        NetworkDenial::HostNotAllowed { .. }
        | NetworkDenial::MissingOrigin
        | NetworkDenial::OriginNotAllowed { .. } => Status::Forbidden,
    }
}

/// Run one request through the whole pipeline.
///
/// The order is fixed and is itself part of the guard: network allowlists, then
/// the request-target grammar, then the health route, then resolution. Nothing
/// downstream can be reached by a request that failed an earlier stage.
pub fn respond(head: &RequestHead<'_>, fs: &FsPolicy, network: &NetworkPolicy) -> Response {
    if let Err(denial) = network.check_host(head.host()) {
        return Response::refusal(status_for_network(&denial));
    }
    if let Err(denial) = network.check_origin(head.method(), head.origin()) {
        return Response::refusal(status_for_network(&denial));
    }
    if head.method() == Method::Options {
        // A preflight that got this far named an allowed origin, but this server
        // has no non-simple endpoints to preflight for, so it advertises none.
        return Response::refusal(Status::MethodNotAllowed);
    }

    let target = match RequestTarget::parse(head.target()) {
        Ok(target) => target,
        Err(error) => return Response::refusal(status_for(&AccessDenied::from(error))),
    };

    if target.path() == HEALTH_TARGET && target.query().is_none() {
        let body = br#"{"status":"ok","engine":"uf-native"}"#.to_vec();
        return Response {
            status: Status::Ok,
            content_type: "application/json",
            loader: "none",
            body: if head.method().wants_body() {
                body
            } else {
                Vec::new()
            },
        };
    }

    match resolve_with_policy(fs, &target) {
        Ok(file) => {
            let content_type = file.media_type().as_str();
            let loader = file.loader().as_str();
            match file.read() {
                Ok(body) => Response {
                    status: Status::Ok,
                    content_type,
                    loader,
                    body: if head.method().wants_body() {
                        body
                    } else {
                        Vec::new()
                    },
                },
                Err(_) => Response::refusal(Status::InternalServerError),
            }
        }
        Err(denied) => Response::refusal(status_for(&denied)),
    }
}
