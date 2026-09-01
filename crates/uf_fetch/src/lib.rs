use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type HeaderList = SmallVec<[FetchHeader; 16]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchClientConfig {
    pub base_url: CompactString,
    pub override_global_fetch: bool,
    pub headers: HeaderList,
}

impl Default for FetchClientConfig {
    fn default() -> Self {
        Self {
            base_url: CompactString::new(""),
            override_global_fetch: false,
            headers: SmallVec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchHeader {
    pub name: CompactString,
    pub value: CompactString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
    pub method: FetchMethod,
    pub path: CompactString,
    pub headers: HeaderList,
}

impl FetchRequest {
    pub fn new(method: FetchMethod, path: impl Into<CompactString>) -> Self {
        Self {
            method,
            path: path.into(),
            headers: SmallVec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FetchMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchClient {
    pub config: FetchClientConfig,
}

impl FetchClient {
    pub fn new(config: FetchClientConfig) -> Self {
        Self { config }
    }

    pub fn request(&self, method: FetchMethod, path: impl Into<CompactString>) -> FetchRequest {
        let mut request = FetchRequest::new(method, path);
        request.headers.extend(self.config.headers.iter().cloned());
        request
    }
}

pub fn ofetch(config: FetchClientConfig) -> FetchClient {
    FetchClient::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_overrides_global_fetch_by_default() {
        let config = FetchClientConfig::default();

        assert!(!config.override_global_fetch);
    }

    #[test]
    fn creates_explicit_request_from_client() {
        let client = ofetch(FetchClientConfig::default());
        let request = client.request(FetchMethod::Get, "/api/user");

        assert_eq!(request.method, FetchMethod::Get);
        assert_eq!(request.path, "/api/user");
    }
}
