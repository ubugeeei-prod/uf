use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_fetch::FetchClientConfig;

pub type GraphQlVariables = SmallVec<[GraphQlVariable; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlClientConfig {
    pub relay_base: bool,
    pub fetch: FetchClientConfig,
}

impl Default for GraphQlClientConfig {
    fn default() -> Self {
        Self {
            relay_base: true,
            fetch: FetchClientConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlOperation {
    pub name: CompactString,
    pub text: CompactString,
    pub variables: GraphQlVariables,
}

impl GraphQlOperation {
    pub fn query(name: impl Into<CompactString>, text: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            variables: SmallVec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlVariable {
    pub name: CompactString,
    pub type_name: CompactString,
}

pub fn graphql(name: impl Into<CompactString>, text: impl Into<CompactString>) -> GraphQlOperation {
    GraphQlOperation::query(name, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_relay_base_without_global_fetch_override() {
        let config = GraphQlClientConfig::default();

        assert!(config.relay_base);
        assert!(!config.fetch.override_global_fetch);
    }

    #[test]
    fn creates_operation_contract() {
        let operation = graphql("ViewerQuery", "query ViewerQuery { viewer { id } }");

        assert_eq!(operation.name, "ViewerQuery");
        assert!(operation.text.contains("viewer"));
    }
}
