//! Template parsing and expansion for destination paths.

use anyhow::{Result, anyhow};
use ariadne::{Color, Label, Report, ReportKind, Source};
use std::collections::HashSet;

/// Known template variables.
const KNOWN_VARIABLES: &[&str] = &[
    // Date/time.
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    // File.
    "filename",
    "extension",
    // EXIF.
    "camera_make",
    "camera_model",
    "lens",
    "iso",
    "focal_length",
];

/// A segment of a parsed template.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal text to include as-is.
    Literal(String),
    /// A variable to be substituted.
    Variable { name: String, span: (usize, usize) },
}

/// A parsed template ready for expansion.
#[derive(Debug, Clone)]
pub struct Template {
    segments: Vec<Segment>,
    source: String,
}

/// Context providing values for template variables.
#[derive(Debug, Default)]
pub struct TemplateContext {
    pub year: String,
    pub month: String,
    pub day: String,
    pub hour: String,
    pub minute: String,
    pub second: String,
    pub filename: String,
    pub extension: String,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<String>,
    pub focal_length: Option<String>,
}

impl Template {
    /// Parse a template string into segments.
    pub fn parse(input: &str) -> Result<Self> {
        let mut segments = Vec::new();
        let mut chars = input.char_indices().peekable();
        let mut literal = String::new();
        let mut errors: Vec<(usize, usize, String)> = Vec::new();

        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    // Escape sequence.
                    if let Some(&(_, next)) = chars.peek() {
                        if next == '{' || next == '}' {
                            chars.next();
                            literal.push(next);
                        } else {
                            // Not a valid escape, keep the backslash.
                            literal.push(c);
                        }
                    } else {
                        literal.push(c);
                    }
                }
                '{' => {
                    // Start of variable.
                    if !literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }

                    let start = i;
                    let mut var_name = String::new();
                    let mut found_close = false;

                    for (j, ch) in chars.by_ref() {
                        if ch == '}' {
                            found_close = true;
                            let end = j + 1;
                            if var_name.is_empty() {
                                errors.push((start, end, "Empty variable name.".to_string()));
                            } else {
                                segments.push(Segment::Variable {
                                    name: var_name,
                                    span: (start, end),
                                });
                            }
                            break;
                        } else if ch == '{' {
                            // Found another '{' before closing '}' - report and return immediately.
                            // Strip trailing non-identifier chars to suggest the likely variable.
                            let likely_var: String = var_name
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect();
                            let suggestion = if likely_var.is_empty() {
                                String::new()
                            } else {
                                format!(": did you mean '{{{}}}'?", likely_var)
                            };
                            let msg = if suggestion.is_empty() {
                                "Missing '}'.".to_string()
                            } else {
                                format!("Missing '}}'{}", suggestion)
                            };
                            Self::report_errors(input, &[(start, j, msg)]);
                            return Err(anyhow!("Failed to parse template."));
                        } else {
                            var_name.push(ch);
                        }
                    }

                    if !found_close && errors.is_empty() {
                        errors.push((start, input.len(), "Missing '}'.".to_string()));
                    }
                }
                '}' => {
                    // Unmatched closing brace.
                    errors.push((i, i + 1, "Unexpected '}': missing '{'.".to_string()));
                }
                _ => {
                    literal.push(c);
                }
            }
        }

        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        if !errors.is_empty() {
            // Show only the first error to avoid confusing cascading errors.
            Self::report_errors(input, &errors[..1]);
            return Err(anyhow!("Failed to parse template."));
        }

        Ok(Self {
            segments,
            source: input.to_string(),
        })
    }

    /// Validate that all variables in the template are known.
    pub fn validate(&self) -> Result<()> {
        let known: HashSet<&str> = KNOWN_VARIABLES.iter().copied().collect();
        let mut errors: Vec<(usize, usize, String)> = Vec::new();

        for segment in &self.segments {
            if let Segment::Variable { name, span } = segment
                && !known.contains(name.as_str())
            {
                errors.push((span.0, span.1, format!("Unknown variable '{}'.", name)));
            }
        }

        if !errors.is_empty() {
            Self::report_errors(&self.source, &errors);

            let available = KNOWN_VARIABLES.join(", ");
            return Err(anyhow!(
                "Template contains unknown variables. Available: {}",
                available
            ));
        }

        Ok(())
    }

    /// Expand the template using the provided context.
    pub fn expand(&self, ctx: &TemplateContext) -> String {
        let mut result = String::new();

        for segment in &self.segments {
            match segment {
                Segment::Literal(s) => result.push_str(s),
                Segment::Variable { name, .. } => {
                    let value = match name.as_str() {
                        "year" => &ctx.year,
                        "month" => &ctx.month,
                        "day" => &ctx.day,
                        "hour" => &ctx.hour,
                        "minute" => &ctx.minute,
                        "second" => &ctx.second,
                        "filename" => &ctx.filename,
                        "extension" => &ctx.extension,
                        "camera_make" => ctx.camera_make.as_deref().unwrap_or("unknown"),
                        "camera_model" => ctx.camera_model.as_deref().unwrap_or("unknown"),
                        "lens" => ctx.lens.as_deref().unwrap_or("unknown"),
                        "iso" => ctx.iso.as_deref().unwrap_or("unknown"),
                        "focal_length" => ctx.focal_length.as_deref().unwrap_or("unknown"),
                        _ => "unknown",
                    };
                    result.push_str(value);
                }
            }
        }

        result
    }

    /// Report parse errors using ariadne.
    fn report_errors(source: &str, errors: &[(usize, usize, String)]) {
        for (start, end, msg) in errors {
            Report::build(ReportKind::Error, ("template", *start..*end))
                .with_message("Invalid template format")
                .with_label(
                    Label::new(("template", *start..*end))
                        .with_message(msg)
                        .with_color(Color::Red),
                )
                .finish()
                .eprint(("template", Source::from(source)))
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let t = Template::parse("{year}/{month}/{day}").unwrap();
        assert_eq!(t.segments.len(), 5);
    }

    #[test]
    fn parse_with_literals() {
        let t = Template::parse("photos/{year}-{month}-{day}/img").unwrap();
        assert_eq!(t.segments.len(), 7);
    }

    #[test]
    fn parse_escaped_braces() {
        let t = Template::parse(r"literal\{brace\}").unwrap();
        assert_eq!(t.segments.len(), 1);
        assert_eq!(
            t.segments[0],
            Segment::Literal("literal{brace}".to_string())
        );
    }

    #[test]
    fn expand_template() {
        let t = Template::parse("{year}/{month}/{filename}.{extension}").unwrap();
        let ctx = TemplateContext {
            year: "2023".to_string(),
            month: "08".to_string(),
            day: "15".to_string(),
            filename: "IMG_1234".to_string(),
            extension: "jpg".to_string(),
            ..Default::default()
        };
        assert_eq!(t.expand(&ctx), "2023/08/IMG_1234.jpg");
    }

    #[test]
    fn validate_unknown_variable() {
        let t = Template::parse("{year}/{unknown}").unwrap();
        assert!(t.validate().is_err());
    }

    #[test]
    fn unclosed_brace_error() {
        assert!(Template::parse("{year").is_err());
    }

    #[test]
    fn unmatched_close_brace_error() {
        assert!(Template::parse("year}").is_err());
    }
}
