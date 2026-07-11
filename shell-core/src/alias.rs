//! Alias management with recursion detection.
//!
//! Stores command aliases and resolves them during expansion,
//! detecting infinite recursion loops.

use std::collections::HashMap;

/// Manages shell command aliases with recursion detection.
#[derive(Debug, Clone)]
pub struct AliasMap {
    aliases: HashMap<String, String>,
}

impl AliasMap {
    /// Creates a new empty alias map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    /// Defines an alias: `name` expands to `value`.
    pub fn insert(&mut self, name: &str, value: &str) {
        self.aliases.insert(name.to_string(), value.to_string());
    }

    /// Removes an alias.
    pub fn remove(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some()
    }

    /// Looks up the raw alias value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(std::string::String::as_str)
    }

    /// Returns whether an alias exists.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }

    /// Returns all alias name/value pairs.
    #[must_use]
    pub fn entries(&self) -> Vec<(&str, &str)> {
        self.aliases
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// Returns the number of aliases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    /// Returns whether the alias map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }

    /// Expands a command name through aliases with recursion detection.
    ///
    /// Follows the alias chain up to `max_depth` levels. Returns the
    /// final resolved command name and any additional arguments produced
    /// by alias expansion. Returns `None` if no alias was found.
    #[must_use]
    pub fn expand(&self, name: &str) -> Option<(String, Vec<String>)> {
        self.expand_with_limit(name, 64)
    }

    fn expand_with_limit(&self, name: &str, max_depth: usize) -> Option<(String, Vec<String>)> {
        let mut visited = Vec::new();
        let mut current = name.to_string();

        for _ in 0..max_depth {
            if !self.contains(&current) {
                break;
            }
            if visited.contains(&current) {
                // Recursion detected — return the current name as-is.
                return Some((current, Vec::new()));
            }
            visited.push(current.clone());
            let expansion = self.get(&current)?;
            let parts: Vec<String> = expansion.split_whitespace().map(str::to_string).collect();
            if let Some((cmd, rest)) = parts.split_first() {
                current = cmd.clone();
                if !rest.is_empty() {
                    return Some((current, rest.to_vec()));
                }
            } else {
                return Some((current, Vec::new()));
            }
        }

        if visited.is_empty() {
            None
        } else {
            Some((current, Vec::new()))
        }
    }
}

impl Default for AliasMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut aliases = AliasMap::new();
        aliases.insert("ll", "ls -la");
        assert_eq!(aliases.get("ll"), Some("ls -la"));
    }

    #[test]
    fn test_remove() {
        let mut aliases = AliasMap::new();
        aliases.insert("ll", "ls -la");
        assert!(aliases.remove("ll"));
        assert!(!aliases.contains("ll"));
    }

    #[test]
    fn test_expand() {
        let mut aliases = AliasMap::new();
        aliases.insert("ll", "ls -la");
        let result = aliases.expand("ll").unwrap();
        assert_eq!(result.0, "ls");
        assert_eq!(result.1, vec!["-la"]);
    }

    #[test]
    fn test_expand_no_alias() {
        let aliases = AliasMap::new();
        assert!(aliases.expand("ls").is_none());
    }

    #[test]
    fn test_expand_chained() {
        let mut aliases = AliasMap::new();
        aliases.insert("l", "ls");
        aliases.insert("ls", "ls --color");
        let result = aliases.expand("l").unwrap();
        assert_eq!(result.0, "ls");
    }

    #[test]
    fn test_expand_recursion_detected() {
        let mut aliases = AliasMap::new();
        aliases.insert("a", "b");
        aliases.insert("b", "a");
        let result = aliases.expand("a").unwrap();
        // Should not loop forever; returns current name.
        assert!(result.0 == "a" || result.0 == "b");
    }

    #[test]
    fn test_entries() {
        let mut aliases = AliasMap::new();
        aliases.insert("l", "ls");
        aliases.insert("g", "grep");
        let entries = aliases.entries();
        assert_eq!(entries.len(), 2);
    }
}
