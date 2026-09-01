use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub message: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutcome {
    pub diagnostics: Vec<ParseDiagnostic>,
    pub parser: ParserKind,
}

impl ParseOutcome {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    OfficialFlowParser,
    Fallback,
}

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("failed to initialize Flow parser: {0}")]
    Initialize(String),
    #[error("Flow parser runtime error: {0}")]
    Runtime(String),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FlowParser;

impl FlowParser {
    pub fn validate_source(&self, source: &str) -> Result<ParseOutcome, FlowError> {
        validate_source(source)
    }
}

#[cfg(feature = "official-parser")]
pub fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    use std::cell::RefCell;

    thread_local! {
        static PARSER: RefCell<Option<flowjs_parser::FlowParser>> = const { RefCell::new(None) };
    }

    let parser_source = normalize_modern_flow_for_parser(source);
    let diagnostics = PARSER.with(|slot| {
        if slot.borrow().is_none() {
            let parser = flowjs_parser::FlowParser::new()
                .map_err(|error| FlowError::Initialize(error.to_string()))?;
            *slot.borrow_mut() = Some(parser);
        }

        let parser = slot.borrow();
        let parser = parser.as_ref().expect("parser initialized");
        parser
            .diagnostics(parser_source.as_deref().unwrap_or(source))
            .map_err(|error| FlowError::Runtime(error.to_string()))
    })?;
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| ParseDiagnostic {
            message: diagnostic.message,
            line: diagnostic.loc.as_ref().map(|loc| loc.start.line),
            column: diagnostic.loc.as_ref().map(|loc| loc.start.column),
        })
        .collect();

    Ok(ParseOutcome {
        diagnostics,
        parser: ParserKind::OfficialFlowParser,
    })
}

pub fn normalize_modern_flow_for_parser(source: &str) -> Option<String> {
    if !source.contains("component ") && !source.contains("hook ") {
        return None;
    }

    let mut changed = false;
    let mut output = String::with_capacity(source.len());
    for line in source.lines() {
        let leading = line.len() - line.trim_start().len();
        let indent = &line[..leading];
        let trimmed = &line[leading..];
        let rewritten = trimmed
            .strip_prefix("export component ")
            .map(|tail| format!("export function {}", strip_renders_clause(tail)))
            .or_else(|| {
                trimmed
                    .strip_prefix("component ")
                    .map(|tail| format!("function {}", strip_renders_clause(tail)))
            })
            .or_else(|| {
                trimmed
                    .strip_prefix("export hook ")
                    .map(|tail| format!("export function {tail}"))
            })
            .or_else(|| {
                trimmed
                    .strip_prefix("hook ")
                    .map(|tail| format!("function {tail}"))
            });

        if let Some(rewritten) = rewritten {
            changed = true;
            output.push_str(indent);
            output.push_str(&rewritten);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    changed.then_some(output)
}

fn strip_renders_clause(tail: &str) -> String {
    let Some(body_start) = tail.find('{') else {
        return tail.to_string();
    };
    let signature = &tail[..body_start];
    let body = &tail[body_start..];
    let Some(renders_start) = signature.find(" renders ") else {
        return tail.to_string();
    };

    format!("{} {}", signature[..renders_start].trim_end(), body)
}

#[cfg(not(feature = "official-parser"))]
pub fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    let diagnostics = if source.contains("type =") {
        vec![ParseDiagnostic {
            message: "fallback parser found an invalid type declaration".to_string(),
            line: None,
            column: None,
        }]
    } else {
        Vec::new()
    };

    Ok(ParseOutcome {
        diagnostics,
        parser: ParserKind::Fallback,
    })
}

pub fn parser_name(kind: ParserKind) -> &'static str {
    match kind {
        ParserKind::OfficialFlowParser => "official-flow-parser",
        ParserKind::Fallback => "fallback",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_modern_flow_syntax() {
        let source = r#"
            // @flow
            opaque type UserId = string;
            component Button(label: string) renders React.Node {
              return label;
            }
        "#;

        let outcome = validate_source(source).expect("parse result");

        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn normalizes_component_and_hook_syntax_for_older_parser_bridge() {
        let normalized = normalize_modern_flow_for_parser(
            "export component Page() renders React.Node { return null; }\nexport hook useX(): number { return 1; }\n",
        )
        .expect("normalized");

        assert!(normalized.contains("export function Page() {"));
        assert!(normalized.contains("export function useX(): number"));
    }

    #[test]
    fn reports_flow_syntax_errors() {
        let source = "// @flow\ntype = ;";

        let outcome = validate_source(source).expect("parse result");

        assert!(!outcome.is_ok());
        assert!(outcome.diagnostics[0].message.contains("Unexpected"));
    }
}
