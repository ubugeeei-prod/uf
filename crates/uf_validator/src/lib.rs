use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type ValidationSteps = SmallVec<[ValidationStep; 8]>;
pub type ValidationIssues = SmallVec<[ValidationIssue; 4]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorSchema {
    pub kind: SchemaKind,
    pub steps: ValidationSteps,
}

impl ValidatorSchema {
    pub fn new(kind: SchemaKind) -> Self {
        Self {
            kind,
            steps: SmallVec::new(),
        }
    }

    pub fn pipe(mut self, step: ValidationStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn validate(&self, value: &ValidationValue) -> ValidationResult {
        let mut issues = ValidationIssues::new();
        if !self.kind.accepts(value) {
            issues.push(ValidationIssue::new(
                "type",
                format!("expected {}", self.kind.name()),
            ));
            return ValidationResult { issues };
        }

        for step in &self.steps {
            if let Some(issue) = step.validate(value) {
                issues.push(issue);
            }
        }

        ValidationResult { issues }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaKind {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Unknown,
}

impl SchemaKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Object => "object",
            Self::Array => "array",
            Self::Unknown => "unknown",
        }
    }

    fn accepts(self, value: &ValidationValue) -> bool {
        matches!(
            (self, value),
            (Self::String, ValidationValue::String(_))
                | (Self::Number, ValidationValue::Number(_))
                | (Self::Boolean, ValidationValue::Boolean(_))
                | (Self::Object, ValidationValue::Object)
                | (Self::Array, ValidationValue::Array)
                | (Self::Unknown, _)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStep {
    MinLength(usize),
    MaxLength(usize),
    StartsWith(CompactString),
    Min(i64),
    Max(i64),
    Integer,
}

impl ValidationStep {
    fn validate(&self, value: &ValidationValue) -> Option<ValidationIssue> {
        match (self, value) {
            (Self::MinLength(min), ValidationValue::String(value))
                if value.chars().count() < *min =>
            {
                Some(ValidationIssue::new(
                    "min_length",
                    format!("expected at least {min} characters"),
                ))
            }
            (Self::MaxLength(max), ValidationValue::String(value))
                if value.chars().count() > *max =>
            {
                Some(ValidationIssue::new(
                    "max_length",
                    format!("expected at most {max} characters"),
                ))
            }
            (Self::StartsWith(prefix), ValidationValue::String(value))
                if !value.starts_with(prefix.as_str()) =>
            {
                Some(ValidationIssue::new(
                    "starts_with",
                    format!("expected prefix {prefix}"),
                ))
            }
            (Self::Min(min), ValidationValue::Number(value)) if value < min => Some(
                ValidationIssue::new("min", format!("expected at least {min}")),
            ),
            (Self::Max(max), ValidationValue::Number(value)) if value > max => Some(
                ValidationIssue::new("max", format!("expected at most {max}")),
            ),
            (Self::Integer, ValidationValue::Number(_)) => None,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationValue {
    String(CompactString),
    Number(i64),
    Boolean(bool),
    Object,
    Array,
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub code: CompactString,
    pub message: CompactString,
}

impl ValidationIssue {
    pub fn new(code: impl Into<CompactString>, message: impl Into<CompactString>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub issues: ValidationIssues,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn string() -> ValidatorSchema {
    ValidatorSchema::new(SchemaKind::String)
}

pub fn number() -> ValidatorSchema {
    ValidatorSchema::new(SchemaKind::Number)
}

pub fn pipe(schema: ValidatorSchema, step: ValidationStep) -> ValidatorSchema {
    schema.pipe(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pipe_steps() {
        let schema = pipe(string(), ValidationStep::MinLength(3))
            .pipe(ValidationStep::StartsWith(CompactString::const_new("uf")));

        let ok = schema.validate(&ValidationValue::String(CompactString::const_new("ufx")));
        let bad = schema.validate(&ValidationValue::String(CompactString::const_new("ux")));

        assert!(ok.is_valid());
        assert_eq!(bad.issues.len(), 2);
    }

    #[test]
    fn reports_type_mismatch_before_steps() {
        let schema = string().pipe(ValidationStep::MinLength(3));
        let result = schema.validate(&ValidationValue::Number(1));

        assert!(!result.is_valid());
        assert_eq!(result.issues[0].code, "type");
    }

    #[test]
    fn validates_number_boundaries() {
        let schema = number()
            .pipe(ValidationStep::Min(1))
            .pipe(ValidationStep::Max(3));

        assert!(schema.validate(&ValidationValue::Number(2)).is_valid());
        assert!(!schema.validate(&ValidationValue::Number(4)).is_valid());
    }
}
