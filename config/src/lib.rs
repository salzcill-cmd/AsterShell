//! Configuration system for `AsterShell`.
//!
//! Loads and validates TOML configuration from `~/.config/aster/config.toml`.
//! Creates a default configuration file when none exists.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use aster_shell_core::{ConfigError, ShellError};
use serde::{Deserialize, Serialize};

/// Root configuration for `AsterShell`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Shell behaviour settings.
    #[serde(default)]
    pub shell: ShellConfig,
    /// Prompt settings.
    #[serde(default)]
    pub prompt: PromptConfig,
    /// History settings.
    #[serde(default)]
    pub history: HistoryConfig,
    /// Theme settings.
    #[serde(default)]
    pub theme: ThemeConfig,
    /// Alias definitions.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    /// Abbreviation definitions (fish-style: expand inline before execution).
    #[serde(default)]
    pub abbreviations: HashMap<String, String>,
    /// Key binding overrides.
    #[serde(default)]
    pub keybindings: HashMap<String, String>,
}

/// General shell settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Whether to print the welcome banner on startup.
    pub welcome_message: bool,
}

/// Prompt display settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    /// Show the last command's exit status indicator.
    pub show_status: bool,
    /// The prompt character displayed after the directory.
    pub symbol: String,
    /// Segments to display in the prompt (e.g. `["user", "dir", "git"]`).
    #[serde(default = "default_segments")]
    pub segments: Vec<String>,
}

/// History configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// Maximum number of history entries to keep.
    pub max_size: usize,
    /// Whether to persist history to disk.
    pub persistent: bool,
    /// Whether to store timestamps with history entries.
    #[serde(default = "default_true")]
    pub timestamps: bool,
}

/// Theme configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Name of the active theme.
    pub name: String,
    /// Enable syntax highlighting in the editor.
    #[serde(default = "default_true")]
    pub syntax_highlighting: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            welcome_message: true,
        }
    }
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            show_status: true,
            symbol: ">".into(),
            segments: default_segments(),
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            persistent: true,
            timestamps: true,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            syntax_highlighting: true,
        }
    }
}

fn default_segments() -> Vec<String> {
    vec!["dir".into()]
}

fn default_true() -> bool {
    true
}

/// Returns the path to the `AsterShell` configuration directory.
///
/// Typically `~/.config/aster/`.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] if the home directory cannot be determined.
pub fn config_dir() -> Result<PathBuf, ShellError> {
    let home = dirs::config_dir().ok_or(ConfigError::MissingHome)?;
    Ok(home.join("aster"))
}

/// Returns the path to the configuration file.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] if the home directory cannot be determined.
pub fn config_file_path() -> Result<PathBuf, ShellError> {
    Ok(config_dir()?.join("config.toml"))
}

/// Returns the path to the history file.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] if the home directory cannot be determined.
pub fn history_file_path() -> Result<PathBuf, ShellError> {
    let data = dirs::data_dir().ok_or(ConfigError::MissingHome)?;
    Ok(data.join("aster").join("history"))
}

/// Ensures the configuration directory and default config file exist.
///
/// If no config file exists, a default one is written.
///
/// # Errors
///
/// Propagates I/O and serialization errors.
pub fn ensure_config() -> Result<Config, ShellError> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| ConfigError::Io {
        path: dir.clone(),
        source: e,
    })?;

    let path = config_file_path()?;
    if path.exists() {
        load_config(&path)
    } else {
        let config = Config::default();
        save_config(&config, &path)?;
        Ok(config)
    }
}

/// Loads configuration from a TOML file.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] or [`ConfigError::Parse`] on failure.
pub fn load_config(path: &Path) -> Result<Config, ShellError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;
    validate_config(&config)?;
    Ok(config)
}

/// Saves configuration to a TOML file.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] or [`ConfigError::Parse`] on failure.
pub fn save_config(config: &Config, path: &Path) -> Result<(), ShellError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let content = toml::to_string_pretty(config).map_err(|e| ConfigError::Parse(e.to_string()))?;
    std::fs::write(path, content).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Validates configuration values are within allowed ranges.
fn validate_config(config: &Config) -> Result<(), ShellError> {
    if config.history.max_size == 0 {
        return Err(ConfigError::InvalidValue {
            key: "history.max_size".into(),
            message: "must be greater than 0".into(),
        }
        .into());
    }
    if config.prompt.symbol.is_empty() {
        return Err(ConfigError::InvalidValue {
            key: "prompt.symbol".into(),
            message: "must not be empty".into(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert!(cfg.shell.welcome_message);
        assert!(cfg.prompt.show_status);
        assert_eq!(cfg.history.max_size, 10_000);
        assert_eq!(cfg.theme.name, "default");
        assert!(cfg.aliases.is_empty());
        assert!(cfg.keybindings.is_empty());
    }

    #[test]
    fn test_validate_config_ok() {
        let cfg = Config::default();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_config_zero_history() {
        let mut cfg = Config::default();
        cfg.history.max_size = 0;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn test_validate_config_empty_symbol() {
        let mut cfg = Config::default();
        cfg.prompt.symbol.clear();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn test_toml_roundtrip() {
        let cfg = Config::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.history.max_size, cfg.history.max_size);
    }

    #[test]
    fn test_config_with_aliases() {
        let mut cfg = Config::default();
        cfg.aliases.insert("ll".into(), "ls -la".into());
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.aliases.get("ll").unwrap(), "ls -la");
    }
}
