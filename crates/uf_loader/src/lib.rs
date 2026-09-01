use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_fetch::{FetchClientConfig, FetchMethod, FetchRequest};
use uf_state::FlowCell;

pub type LoaderDeps = SmallVec<[CompactString; 8]>;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loader<T> {
    pub key: CompactString,
    pub request: FetchRequest,
    pub state: FlowCell<LoaderState<T>>,
    pub deps: LoaderDeps,
    pub cache: LoaderCacheMode,
}

impl<T> Loader<T> {
    pub fn new(key: impl Into<CompactString>, request: FetchRequest) -> Self {
        let key = key.into();
        Self {
            state: FlowCell::new(key.as_str(), LoaderState::Idle),
            key,
            request,
            deps: SmallVec::new(),
            cache: LoaderCacheMode::OptIn,
        }
    }

    pub fn depends_on(mut self, dep: impl Into<CompactString>) -> Self {
        self.deps.push(dep.into());
        self
    }

    pub fn snapshot(&self) -> &LoaderState<T> {
        self.state.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoaderState<T> {
    Idle,
    Pending,
    Ready(T),
    Failed(CompactString),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoaderCacheMode {
    OptIn,
}

pub fn json_loader<T>(key: impl Into<CompactString>, path: impl Into<CompactString>) -> Loader<T> {
    Loader::new(key, FetchRequest::new(FetchMethod::Get, path))
}

pub fn fetch_config() -> FetchClientConfig {
    FetchClientConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_flow_cell_backed_loader() {
        let loader = json_loader::<String>("user", "/api/user");

        assert_eq!(loader.key, "user");
        assert_eq!(loader.request.path, "/api/user");
        assert_eq!(loader.snapshot(), &LoaderState::Idle);
        assert_eq!(loader.cache, LoaderCacheMode::OptIn);
    }

    #[test]
    fn tracks_dependencies_for_rerun_graphs() {
        let loader = json_loader::<String>("user", "/api/user").depends_on("session");

        assert_eq!(loader.deps.len(), 1);
        assert_eq!(loader.deps[0], "session");
    }

    #[test]
    fn inherits_fetch_no_global_override_default() {
        assert!(!fetch_config().override_global_fetch);
    }
}
