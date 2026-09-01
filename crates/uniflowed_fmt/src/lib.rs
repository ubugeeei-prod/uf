use thiserror::Error;
use uniflowed_config::FmtConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    pub output: String,
    pub changed: bool,
}

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("indent width must be greater than zero")]
    InvalidIndentWidth,
}

pub fn format_source(source: &str, config: &FmtConfig) -> Result<FormatResult, FormatError> {
    if config.indent_width == 0 {
        return Err(FormatError::InvalidIndentWidth);
    }

    let indent = " ".repeat(config.indent_width as usize);
    let mut output = String::with_capacity(source.len() + 1);

    for line in source.lines() {
        let trimmed_end = line.trim_end_matches([' ', '\t']);
        let leading_tabs = trimmed_end
            .bytes()
            .take_while(|byte| *byte == b'\t')
            .count();

        if leading_tabs > 0 {
            output.push_str(&indent.repeat(leading_tabs));
            output.push_str(&trimmed_end[leading_tabs..]);
        } else {
            output.push_str(trimmed_end);
        }
        output.push('\n');
    }

    if source.is_empty() {
        output.clear();
    } else if !source.ends_with('\n') && output.is_empty() {
        output.push('\n');
    }

    Ok(FormatResult {
        changed: output != source,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_trailing_whitespace_and_adds_final_newline() {
        let config = FmtConfig::default();

        let result = format_source("const x = 1;  \nconst y = 2;", &config).unwrap();

        similar_asserts::assert_eq!(result.output, "const x = 1;\nconst y = 2;\n");
        assert!(result.changed);
    }

    #[test]
    fn expands_leading_tabs_using_configured_indent() {
        let config = FmtConfig {
            indent_width: 4,
            ..FmtConfig::default()
        };

        let result = format_source("\tconst x = 1;\n", &config).unwrap();

        similar_asserts::assert_eq!(result.output, "    const x = 1;\n");
    }

    #[test]
    fn formatting_is_idempotent() {
        let config = FmtConfig::default();
        let once = format_source("const x = 1;\n", &config).unwrap();
        let twice = format_source(&once.output, &config).unwrap();

        assert!(!once.changed);
        assert!(!twice.changed);
    }
}
