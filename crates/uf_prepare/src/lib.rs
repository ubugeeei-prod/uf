use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type PrepareSteps = SmallVec<[PrepareStep; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparePlan {
    pub lint_staged_compatible: bool,
    pub code_generator: bool,
    pub write_generated_files: bool,
    pub cache: PrepareCacheMode,
    pub steps: PrepareSteps,
}

impl Default for PreparePlan {
    fn default() -> Self {
        Self {
            lint_staged_compatible: true,
            code_generator: true,
            write_generated_files: true,
            cache: PrepareCacheMode::OptIn,
            steps: smallvec::smallvec![
                PrepareStep::DiscoverStagedFiles,
                PrepareStep::GenerateRouterTypes,
                PrepareStep::GenerateServerActionTypes,
                PrepareStep::GenerateValidatorTypes,
                PrepareStep::RunLint,
                PrepareStep::RunFormatCheck,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrepareStep {
    DiscoverStagedFiles,
    GenerateRouterTypes,
    GenerateServerActionTypes,
    GenerateValidatorTypes,
    RunLint,
    RunFormatCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrepareCacheMode {
    OptIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: CompactString,
    pub kind: GeneratedFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratedFileKind {
    RouterTypes,
    ServerActionTypes,
    ValidatorTypes,
}

pub fn default_plan() -> PreparePlan {
    PreparePlan::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prepare_plan_runs_lint_staged_and_generators() {
        let plan = default_plan();

        assert!(plan.lint_staged_compatible);
        assert!(plan.code_generator);
        assert!(plan.write_generated_files);
        assert_eq!(plan.cache, PrepareCacheMode::OptIn);
        assert!(plan.steps.contains(&PrepareStep::DiscoverStagedFiles));
        assert!(plan.steps.contains(&PrepareStep::GenerateRouterTypes));
        assert!(plan.steps.contains(&PrepareStep::GenerateValidatorTypes));
    }
}
