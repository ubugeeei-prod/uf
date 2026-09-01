use compact_str::CompactString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownEngineContract {
    pub engine: MarkdownEngine,
    pub wasm_module: CompactString,
    pub rsc_safe: bool,
    pub cache: MarkdownCacheMode,
}

impl Default for MarkdownEngineContract {
    fn default() -> Self {
        Self {
            engine: MarkdownEngine::OxContentWasm,
            wasm_module: CompactString::const_new("ox-content"),
            rsc_safe: true,
            cache: MarkdownCacheMode::OptIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkdownEngine {
    OxContentWasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkdownCacheMode {
    OptIn,
}

pub fn contract() -> MarkdownEngineContract {
    MarkdownEngineContract::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_ox_content_wasm_and_opt_in_cache() {
        let contract = contract();

        assert_eq!(contract.engine, MarkdownEngine::OxContentWasm);
        assert_eq!(contract.wasm_module, "ox-content");
        assert!(contract.rsc_safe);
        assert_eq!(contract.cache, MarkdownCacheMode::OptIn);
    }
}
