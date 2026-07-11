use crate::error::{self, ErrorKind, ShellError};
use crate::jobs::JobManager;
use crate::span::Span;
use std::collections::HashMap;

/// Handles shell function definitions and invocations.
pub struct FunctionManager {
    functions: HashMap<String, Function>,
    #[allow(dead_code)]
    job_manager: std::sync::Arc<JobManager>,
}

struct Function {
    body: Vec<crate::ast::Statement>,
    #[allow(dead_code)]
    span: Span,
}

impl FunctionManager {
    /// Creates a new `FunctionManager`.
    #[must_use]
    pub fn new(job_manager: std::sync::Arc<JobManager>) -> Self {
        Self {
            functions: HashMap::new(),
            job_manager,
        }
    }

    /// Defines a new shell function.
    pub fn define(
        &mut self,
        name: String,
        body: Vec<crate::ast::Statement>,
        span: Span,
    ) -> Result<(), ShellError> {
        if !error::is_valid_identifier(&name) {
            return Err(ShellError::new(
                ErrorKind::InvalidFunctionName,
                format!("not a valid function name: `{name}`"),
                span,
            ));
        }
        self.functions.insert(name, Function { body, span });
        Ok(())
    }

    /// Checks if a function is defined.
    #[must_use]
    pub fn is_defined(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Returns the body of a function.
    #[must_use]
    pub fn get_body(&self, name: &str) -> Option<&[crate::ast::Statement]> {
        self.functions.get(name).map(|f| f.body.as_slice())
    }

    /// Returns the names of all defined functions.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.functions.keys().map(String::as_str).collect()
    }

    /// Returns the number of defined functions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Returns true if no functions are defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Lists all defined functions.
    pub fn list(&self) -> Vec<(String, &str)> {
        self.functions
            .iter()
            .map(|(name, f)| (name.clone(), &f.body))
            .map(|(name, body)| {
                let desc = if body.len() == 1 {
                    "1 statement"
                } else {
                    "complex body"
                };
                (name, desc)
            })
            .collect()
    }

    /// Removes a function and returns true if it existed.
    pub fn remove(&mut self, name: &str) -> bool {
        self.functions.remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(1, 1, 0, 5)
    }

    #[test]
    fn test_define_and_get() {
        let mut mgr = FunctionManager::new(std::sync::Arc::new(JobManager::new()));
        let body = vec![crate::ast::Statement::Break(dummy_span())];
        mgr.define("myfunc".into(), body, dummy_span()).unwrap();
        assert!(mgr.is_defined("myfunc"));
        assert!(!mgr.is_defined("other"));
    }

    #[test]
    fn test_remove() {
        let mut mgr = FunctionManager::new(std::sync::Arc::new(JobManager::new()));
        let body = vec![crate::ast::Statement::Break(dummy_span())];
        mgr.define("f".into(), body, dummy_span()).unwrap();
        assert!(mgr.remove("f"));
        assert!(!mgr.remove("f"));
        assert!(!mgr.is_defined("f"));
    }

    #[test]
    fn test_list() {
        let mut mgr = FunctionManager::new(std::sync::Arc::new(JobManager::new()));
        let body = vec![crate::ast::Statement::Break(dummy_span())];
        mgr.define("alpha".into(), body, dummy_span()).unwrap();
        let names = mgr.names();
        assert!(names.contains(&"alpha"));
    }
}
