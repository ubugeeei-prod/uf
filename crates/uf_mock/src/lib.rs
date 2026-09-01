use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type MockHandlers = SmallVec<[MockHandler; 16]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockRegistry {
    pub handlers: MockHandlers,
    pub unmatched: UnmatchedRequestPolicy,
}

impl Default for MockRegistry {
    fn default() -> Self {
        Self {
            handlers: SmallVec::new(),
            unmatched: UnmatchedRequestPolicy::Error,
        }
    }
}

impl MockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handler(mut self, handler: MockHandler) -> Self {
        self.handlers.push(handler);
        self
    }

    pub fn resolve(&self, method: HttpMethod, path: &str) -> Option<&MockHandler> {
        self.handlers
            .iter()
            .find(|handler| handler.method == method && handler.route.matches(path))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockHandler {
    pub method: HttpMethod,
    pub route: MockRoute,
    pub response: MockResponse,
}

impl MockHandler {
    pub fn json(method: HttpMethod, path: impl Into<CompactString>, status: u16) -> Self {
        Self {
            method,
            route: MockRoute::new(path),
            response: MockResponse {
                status,
                body: MockBody::Json,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockRoute {
    pub path: CompactString,
}

impl MockRoute {
    pub fn new(path: impl Into<CompactString>) -> Self {
        Self { path: path.into() }
    }

    pub fn matches(&self, path: &str) -> bool {
        self.path == path
            || self.path.ends_with("/*") && path.starts_with(&self.path[..self.path.len() - 1])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MockBody {
    Empty,
    Json,
    Text,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockResponse {
    pub status: u16,
    pub body: MockBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnmatchedRequestPolicy {
    Error,
    Passthrough,
}

pub fn get(path: impl Into<CompactString>, status: u16) -> MockHandler {
    MockHandler::json(HttpMethod::Get, path, status)
}

pub fn post(path: impl Into<CompactString>, status: u16) -> MockHandler {
    MockHandler::json(HttpMethod::Post, path, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_exact_handler() {
        let registry = MockRegistry::new().handler(get("/api/user", 200));
        let handler = registry
            .resolve(HttpMethod::Get, "/api/user")
            .expect("handler");

        assert_eq!(handler.response.status, 200);
    }

    #[test]
    fn resolves_wildcard_handler() {
        let registry = MockRegistry::new().handler(post("/api/*", 201));

        assert!(registry.resolve(HttpMethod::Post, "/api/action").is_some());
        assert!(registry.resolve(HttpMethod::Get, "/api/action").is_none());
    }
}
