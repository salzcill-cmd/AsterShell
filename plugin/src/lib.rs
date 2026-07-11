//! Configuration-driven plugin system for `AsterShell`.
//!
//! Plugins are TOML files (`.aster`) placed in `~/.config/aster/plugins/`.
//! Each plugin declares metadata, shell commands to source, aliases, and
//! optional inter-plugin dependencies.  The [`PluginManager`] discovers,
//! loads, enables, disables, and removes these files so that the shell
//! runtime can source the appropriate scripts at startup.
//!
//! # Plugin format
//!
//! ```toml
//! name        = "git-utils"
//! version     = "0.1.0"
//! description = "Handy git aliases and functions"
//! enabled     = true
//! commands    = ["git-utils/init.sh", "git-utils/aliases.sh"]
//! dependencies = ["base-utils"]
//!
//! [aliases]
//! gs = "git status"
//! gd = "git diff"
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use aster_shell_core::PluginError;
use aster_shell_core::ShellError;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// A single plugin definition, deserialised from a `.aster` TOML file.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Plugin {
    /// Unique name of the plugin.
    pub name: String,
    /// `SemVer` version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Whether this plugin is active.
    pub enabled: bool,
    /// Shell scripts (relative to the plugin dir) that should be sourced.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Names of other plugins that must be loaded first.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Shell aliases the plugin registers.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// PluginManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all installed plugins.
#[derive(Debug)]
pub struct PluginManager {
    /// Absolute path to the plugins directory.
    plugins_dir: PathBuf,
    /// All loaded plugins, in discovery order.
    plugins: Vec<Plugin>,
}

impl PluginManager {
    /// Creates a manager that targets the default plugin directory
    /// (`~/.config/aster/plugins/`).
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::MissingPluginDir`] when the home directory
    /// cannot be determined.
    pub fn new() -> Result<Self, ShellError> {
        let dir = Self::default_plugins_dir()?;
        Ok(Self::with_dir(dir))
    }

    /// Creates a manager that targets `dir`, creating it if it does not exist.
    #[must_use]
    pub const fn with_dir(dir: PathBuf) -> Self {
        Self {
            plugins_dir: dir,
            plugins: Vec::new(),
        }
    }

    /// Returns the default plugins directory (`~/.config/aster/plugins/`).
    fn default_plugins_dir() -> Result<PathBuf, ShellError> {
        let home = dirs::config_dir().ok_or(PluginError::MissingPluginDir)?;
        Ok(home.join("aster").join("plugins"))
    }

    // -- persistence --------------------------------------------------------

    /// Ensures the plugins directory exists on disk.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::Io`] if the directory cannot be created.
    pub fn ensure_dir(&self) -> Result<(), ShellError> {
        if !self.plugins_dir.exists() {
            fs::create_dir_all(&self.plugins_dir).map_err(|e| {
                ShellError::Plugin(PluginError::Io {
                    path: self.plugins_dir.clone(),
                    source: e,
                })
            })?;
        }
        Ok(())
    }

    /// Discovers and loads every `.aster` file in the plugins directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugins directory cannot be read.  Individual
    /// files that fail to parse are silently skipped (with a log message).
    pub fn load_all(&mut self) -> Result<(), ShellError> {
        self.ensure_dir()?;

        let entries = fs::read_dir(&self.plugins_dir).map_err(|e| {
            ShellError::Plugin(PluginError::Io {
                path: self.plugins_dir.clone(),
                source: e,
            })
        })?;

        self.plugins.clear();

        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "aster"))
            .collect();

        paths.sort();

        for path in paths {
            match Self::parse_plugin_file(&path) {
                Ok(plugin) => {
                    tracing::debug!(name = %plugin.name, version = %plugin.version, "loaded plugin");
                    log::debug!("loaded plugin {}", plugin.name);
                    self.plugins.push(plugin);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping invalid plugin");
                    log::warn!("skipping invalid plugin {}: {e}", path.display());
                }
            }
        }

        Ok(())
    }

    /// Parses a single `.aster` file into a [`Plugin`].
    fn parse_plugin_file(path: &Path) -> Result<Plugin, ShellError> {
        let content = fs::read_to_string(path).map_err(|e| {
            ShellError::Plugin(PluginError::Io {
                path: path.to_path_buf(),
                source: e,
            })
        })?;

        let plugin: Plugin = toml::from_str(&content).map_err(|e| {
            ShellError::Plugin(PluginError::Parse {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })
        })?;

        Self::validate_name(&plugin.name)?;

        Ok(plugin)
    }

    /// Rejects names that are empty or contain path separators.
    fn validate_name(name: &str) -> Result<(), ShellError> {
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(ShellError::Plugin(PluginError::InvalidName(
                name.to_string(),
            )));
        }
        Ok(())
    }

    // -- queries ------------------------------------------------------------

    /// Returns a reference to every loaded plugin, in discovery order.
    #[must_use]
    pub fn list(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Returns only the enabled plugins, in discovery order.
    #[must_use]
    pub fn enabled(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.enabled).collect()
    }

    /// Returns only the disabled plugins, in discovery order.
    #[must_use]
    pub fn disabled(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| !p.enabled).collect()
    }

    /// Finds a plugin by exact name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// Finds a plugin by exact name (mutable).
    fn get_mut(&mut self, name: &str) -> Option<&mut Plugin> {
        self.plugins.iter_mut().find(|p| p.name == name)
    }

    /// Returns the paths of all shell scripts that should be sourced for the
    /// currently enabled plugins, in discovery order.
    #[must_use]
    pub fn source_scripts(&self) -> Vec<PathBuf> {
        self.enabled()
            .into_iter()
            .flat_map(|p| p.commands.iter().map(move |cmd| self.plugins_dir.join(cmd)))
            .collect()
    }

    /// Returns the merged set of aliases from all enabled plugins.
    #[must_use]
    pub fn aliases(&self) -> HashMap<&str, &str> {
        let mut map = HashMap::new();
        for p in self.enabled() {
            for (k, v) in &p.aliases {
                map.insert(k.as_str(), v.as_str());
            }
        }
        map
    }

    // -- mutations ----------------------------------------------------------

    /// Enables a plugin by name and persists the change to disk.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::NotFound`] if no plugin with `name` is loaded.
    pub fn enable(&mut self, name: &str) -> Result<(), ShellError> {
        let Some(plugin) = self.get_mut(name) else {
            return Err(ShellError::Plugin(PluginError::NotFound(name.to_string())));
        };

        plugin.enabled = true;
        let snapshot = plugin.clone();
        self.persist(&snapshot)
    }

    /// Disables a plugin by name and persists the change to disk.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::NotFound`] if no plugin with `name` is loaded.
    pub fn disable(&mut self, name: &str) -> Result<(), ShellError> {
        let Some(plugin) = self.get_mut(name) else {
            return Err(ShellError::Plugin(PluginError::NotFound(name.to_string())));
        };

        plugin.enabled = false;
        let snapshot = plugin.clone();
        self.persist(&snapshot)
    }

    /// Removes a plugin from memory **and** deletes its `.aster` file from
    /// disk.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::NotFound`] if no plugin with `name` is loaded,
    /// or [`PluginError::Io`] if the file cannot be deleted.
    pub fn remove(&mut self, name: &str) -> Result<(), ShellError> {
        let idx = self
            .plugins
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| ShellError::Plugin(PluginError::NotFound(name.to_string())))?;

        let plugin = &self.plugins[idx];
        let path = self.plugin_path(&plugin.name);

        if path.exists() {
            fs::remove_file(&path).map_err(|e| {
                ShellError::Plugin(PluginError::Io {
                    path: path.clone(),
                    source: e,
                })
            })?;
        }

        self.plugins.remove(idx);
        Ok(())
    }

    /// Adds a brand-new plugin from a [`Plugin`] value, writing its `.aster`
    /// file to disk.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::AlreadyLoaded`] if a plugin with the same name
    /// is already loaded.
    pub fn add(&mut self, plugin: Plugin) -> Result<(), ShellError> {
        if self.get(&plugin.name).is_some() {
            return Err(ShellError::Plugin(PluginError::AlreadyLoaded(plugin.name)));
        }

        self.ensure_dir()?;
        self.persist(&plugin)?;
        self.plugins.push(plugin);
        Ok(())
    }

    /// Returns the number of loaded plugins.
    #[must_use]
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Returns the number of enabled plugins.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.enabled).count()
    }

    // -- internal -----------------------------------------------------------

    /// Builds the file-system path for a plugin with the given name.
    #[must_use]
    fn plugin_path(&self, name: &str) -> PathBuf {
        self.plugins_dir.join(format!("{name}.aster"))
    }

    /// Serialises a [`Plugin`] to TOML and writes it to its `.aster` file.
    fn persist(&self, plugin: &Plugin) -> Result<(), ShellError> {
        let path = self.plugin_path(&plugin.name);
        let content = toml::to_string_pretty(plugin).map_err(|e| {
            ShellError::Plugin(PluginError::Parse {
                path: path.clone(),
                reason: e.to_string(),
            })
        })?;

        fs::write(&path, &content)
            .map_err(|e| ShellError::Plugin(PluginError::Io { path, source: e }))?;

        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        // If the default directory cannot be resolved we fall back to a
        // no-plugins manager so that `Default` never fails.
        Self::default_plugins_dir().map_or_else(
            |_| Self::with_dir(PathBuf::from("/dev/null")),
            Self::with_dir,
        )
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a [`PluginManager`] backed by a temporary directory.
    fn temp_manager() -> (tempfile::TempDir, PluginManager) {
        let td = tempfile::TempDir::new().expect("tempdir");
        let pm = PluginManager::with_dir(td.path().to_path_buf());
        (td, pm)
    }

    /// Write a `.aster` file directly into the temp dir.
    fn write_plugin_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(format!("{name}.aster")), content).expect("write plugin file");
    }

    fn sample_toml(name: &str, enabled: bool) -> String {
        format!(
            r#"
name        = "{name}"
version     = "0.1.0"
description = "test plugin {name}"
enabled     = {enabled}
commands    = ["{name}/init.sh"]
aliases     = {{ gp = "git pull" }}
"#
        )
    }

    // -- load ----------------------------------------------------------------

    #[test]
    fn load_all_discovers_aster_files() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "alpha", &sample_toml("alpha", true));
        write_plugin_file(td.path(), "beta", &sample_toml("beta", false));

        pm.load_all().expect("load_all");
        assert_eq!(pm.count(), 2);
    }

    #[test]
    fn load_all_skips_non_aster_files() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "good", &sample_toml("good", true));
        fs::write(td.path().join("notes.txt"), "not a plugin").unwrap();

        pm.load_all().expect("load_all");
        assert_eq!(pm.count(), 1);
    }

    #[test]
    fn load_all_skips_invalid_toml() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "broken", "this is not valid toml {{{");

        pm.load_all().expect("load_all");
        assert_eq!(pm.count(), 0);
    }

    #[test]
    fn load_all_creates_dir_if_missing() {
        let td = tempfile::TempDir::new().expect("tempdir");
        let nested = td.path().join("deep").join("nested");
        let mut pm = PluginManager::with_dir(nested.clone());

        pm.load_all().expect("load_all should create dir");
        assert!(nested.exists());
    }

    // -- queries ------------------------------------------------------------

    #[test]
    fn list_returns_all() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "a", &sample_toml("a", true));
        write_plugin_file(td.path(), "b", &sample_toml("b", false));

        pm.load_all().unwrap();
        assert_eq!(pm.list().len(), 2);
    }

    #[test]
    fn enabled_filters_correctly() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "on", &sample_toml("on", true));
        write_plugin_file(td.path(), "off", &sample_toml("off", false));

        pm.load_all().unwrap();
        assert_eq!(pm.enabled().len(), 1);
        assert_eq!(pm.enabled()[0].name, "on");
    }

    #[test]
    fn disabled_filters_correctly() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "on", &sample_toml("on", true));
        write_plugin_file(td.path(), "off", &sample_toml("off", false));

        pm.load_all().unwrap();
        assert_eq!(pm.disabled().len(), 1);
        assert_eq!(pm.disabled()[0].name, "off");
    }

    #[test]
    fn get_by_name() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "findme", &sample_toml("findme", true));

        pm.load_all().unwrap();
        assert!(pm.get("findme").is_some());
        assert!(pm.get("nope").is_none());
    }

    #[test]
    fn count_and_enabled_count() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "a", &sample_toml("a", true));
        write_plugin_file(td.path(), "b", &sample_toml("b", false));
        write_plugin_file(td.path(), "c", &sample_toml("c", true));

        pm.load_all().unwrap();
        assert_eq!(pm.count(), 3);
        assert_eq!(pm.enabled_count(), 2);
    }

    // -- enable / disable ---------------------------------------------------

    #[test]
    fn enable_plugin() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "toggle", &sample_toml("toggle", false));
        pm.load_all().unwrap();

        assert!(!pm.get("toggle").unwrap().enabled);
        pm.enable("toggle").unwrap();
        assert!(pm.get("toggle").unwrap().enabled);

        // Verify persisted on disk.
        let reloaded = reload_from_disk(&td);
        assert!(reloaded.get("toggle").unwrap().enabled);
    }

    #[test]
    fn disable_plugin() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "toggle", &sample_toml("toggle", true));
        pm.load_all().unwrap();

        pm.disable("toggle").unwrap();
        assert!(!pm.get("toggle").unwrap().enabled);

        let reloaded = reload_from_disk(&td);
        assert!(!reloaded.get("toggle").unwrap().enabled);
    }

    #[test]
    fn enable_nonexistent_returns_error() {
        let (_td, mut pm) = temp_manager();
        assert!(pm.enable("ghost").is_err());
    }

    #[test]
    fn disable_nonexistent_returns_error() {
        let (_td, mut pm) = temp_manager();
        assert!(pm.disable("ghost").is_err());
    }

    // -- remove -------------------------------------------------------------

    #[test]
    fn remove_deletes_file_and_memory() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "doomed", &sample_toml("doomed", true));
        pm.load_all().unwrap();

        pm.remove("doomed").unwrap();
        assert_eq!(pm.count(), 0);
        assert!(!td.path().join("doomed.aster").exists());
    }

    #[test]
    fn remove_nonexistent_returns_error() {
        let (_td, mut pm) = temp_manager();
        assert!(pm.remove("ghost").is_err());
    }

    // -- add ----------------------------------------------------------------

    #[test]
    fn add_plugin() {
        let (_td, mut pm) = temp_manager();
        let plugin = Plugin {
            name: "new".into(),
            version: "1.0.0".into(),
            description: "brand new".into(),
            enabled: true,
            commands: vec!["setup.sh".into()],
            dependencies: vec![],
            aliases: HashMap::from([("ll".into(), "ls -la".into())]),
        };

        pm.add(plugin).unwrap();
        assert_eq!(pm.count(), 1);
        assert_eq!(pm.get("new").unwrap().version, "1.0.0");
    }

    #[test]
    fn add_duplicate_returns_error() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "dup", &sample_toml("dup", true));
        pm.load_all().unwrap();

        let plugin = Plugin {
            name: "dup".into(),
            version: "2.0.0".into(),
            description: "duplicate".into(),
            enabled: true,
            commands: vec![],
            dependencies: vec![],
            aliases: HashMap::new(),
        };

        assert!(pm.add(plugin).is_err());
    }

    // -- source_scripts / aliases -------------------------------------------

    #[test]
    fn source_scripts_returns_enabled_only() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "on", &sample_toml("on", true));
        write_plugin_file(td.path(), "off", &sample_toml("off", false));
        pm.load_all().unwrap();

        let scripts = pm.source_scripts();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].to_string_lossy().contains("on/init.sh"));
    }

    #[test]
    fn aliases_merge_from_enabled() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "a", &sample_toml("a", true));
        write_plugin_file(td.path(), "b", &sample_toml("b", false));
        pm.load_all().unwrap();

        let aliases = pm.aliases();
        assert_eq!(aliases.len(), 1);
        assert_eq!(*aliases.get("gp").unwrap(), "git pull");
    }

    // -- default dir --------------------------------------------------------

    #[test]
    fn default_manager_has_correct_dir() {
        let pm = PluginManager::default();
        assert!(pm.plugins_dir.to_string_lossy().contains("aster"));
        assert!(pm.plugins_dir.to_string_lossy().contains("plugins"));
    }

    // -- persistence round-trip ---------------------------------------------

    #[test]
    fn persist_round_trip_preserves_fields() {
        let (td, mut pm) = temp_manager();
        let plugin = Plugin {
            name: "roundtrip".into(),
            version: "3.2.1".into(),
            description: "round-trip test".into(),
            enabled: false,
            commands: vec!["a.sh".into(), "b.sh".into()],
            dependencies: vec!["dep1".into()],
            aliases: HashMap::from([("ll".into(), "ls -la".into())]),
        };

        pm.add(plugin).unwrap();

        let reloaded = reload_from_disk(&td);
        let p = reloaded.get("roundtrip").unwrap();
        assert_eq!(p.version, "3.2.1");
        assert!(!p.enabled);
        assert_eq!(p.commands, vec!["a.sh", "b.sh"]);
        assert_eq!(p.dependencies, vec!["dep1"]);
        assert_eq!(p.aliases.get("ll").unwrap(), "ls -la");
    }

    // -- invalid names ------------------------------------------------------

    #[test]
    fn empty_name_is_rejected() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "x", &sample_toml("", true));
        pm.load_all().unwrap();
        assert_eq!(pm.count(), 0);
    }

    #[test]
    fn name_with_slash_is_rejected() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "x", &sample_toml("bad/name", true));
        pm.load_all().unwrap();
        assert_eq!(pm.count(), 0);
    }

    // -- discovery order ----------------------------------------------------

    #[test]
    fn plugins_are_loaded_in_sorted_order() {
        let (td, mut pm) = temp_manager();
        write_plugin_file(td.path(), "z.Plugin", &sample_toml("z", true));
        write_plugin_file(td.path(), "a.Plugin", &sample_toml("a", true));
        write_plugin_file(td.path(), "m.Plugin", &sample_toml("m", true));

        pm.load_all().unwrap();
        let names: Vec<&str> = pm.list().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["a", "m", "z"]);
    }

    // -- ensure_dir ---------------------------------------------------------

    #[test]
    fn ensure_dir_creates_directory() {
        let td = tempfile::TempDir::new().unwrap();
        let nested = td.path().join("new").join("dir");
        let pm = PluginManager::with_dir(nested.clone());

        pm.ensure_dir().unwrap();
        assert!(nested.exists());
    }

    // -- default with missing home ------------------------------------------

    #[test]
    fn default_with_missing_home_uses_fallback() {
        // Just verify `Default::default()` doesn't panic even if HOME is
        // unset.
        let _pm = PluginManager::default();
    }

    // -- helpers ------------------------------------------------------------

    /// Reload plugins from the same temp directory to verify disk state.
    fn reload_from_disk(td: &tempfile::TempDir) -> PluginManager {
        let mut pm = PluginManager::with_dir(td.path().to_path_buf());
        pm.load_all().unwrap();
        pm
    }
}
