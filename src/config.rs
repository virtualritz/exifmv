//! Configuration file loading and management.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default format string for destination paths.
pub const DEFAULT_FORMAT: &str = "{year}/{month}/{day}/{filename}.{extension}";

/// Application name for confy.
const APP_NAME: &str = "exifmv";

/// Configuration loaded from TOML file.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// Path format template.
    pub format: Option<String>,
    /// Change filename & extension to lowercase.
    pub make_lowercase: Option<bool>,
    /// Recurse subdirectories.
    pub recursive: Option<bool>,
    /// Time at which date wraps to next day.
    pub day_wrap: Option<String>,
    /// Verbose output.
    pub verbose: Option<bool>,
    /// Exit on first error.
    pub halt_on_errors: Option<bool>,
    /// Follow symbolic links.
    pub dereference: Option<bool>,
    /// Use checksum for duplicate detection instead of size.
    pub checksum: Option<bool>,
}

impl Config {
    /// Load config from the given path, or the default path if `None`.
    /// Returns default config if file doesn't exist.
    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        if let Some(path) = path {
            Ok(confy::load_path(path)?)
        } else {
            Ok(confy::load(APP_NAME, "config")?)
        }
    }

    /// Returns the format string, using default if not specified.
    pub fn format(&self) -> &str {
        self.format.as_deref().unwrap_or(DEFAULT_FORMAT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        let toml = r#"
format = "{year}-{month}-{day}/{filename}.{extension}"
make-lowercase = true
day-wrap = "04:00"
verbose = false
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            config.format.as_deref(),
            Some("{year}-{month}-{day}/{filename}.{extension}")
        );
        assert_eq!(config.make_lowercase, Some(true));
        assert_eq!(config.day_wrap.as_deref(), Some("04:00"));
        assert_eq!(config.verbose, Some(false));
    }

    #[test]
    fn empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.format.is_none());
        assert!(config.make_lowercase.is_none());
    }
}
