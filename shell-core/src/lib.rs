//! Core types, traits, and error system for `AsterShell`.
//!
//! This crate provides the foundational types shared across all other
//! `AsterShell` crates: the abstract syntax tree, source spans, and
//! the unified error hierarchy.

/// Alias management with recursion detection.
pub mod alias;
/// Abstract syntax tree types for shell programs.
pub mod ast;
/// Directory stack for `pushd` / `popd` / `dirs`.
pub mod dirstack;
/// Environment variable management.
pub mod environment;
/// Error types used across the shell.
pub mod error;
/// Shell function management.
pub mod functions;
/// Glob pattern expansion.
pub mod glob;
/// Job control and background process management.
pub mod jobs;
/// Source location spans.
pub mod span;

pub use alias::AliasMap;
pub use ast::*;
pub use dirstack::DirectoryStack;
pub use environment::ShellEnvironment;
pub use error::*;
pub use span::*;

/// The current version of `AsterShell`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The name of the shell.
pub const SHELL_NAME: &str = "aster";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_shell_name_constant() {
        assert_eq!(SHELL_NAME, "aster");
    }
}
