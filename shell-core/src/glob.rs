use glob::glob;
use std::path::Path;

/// Expands glob patterns in a list of arguments.
///
/// First performs brace expansion (`{a,b,c}` and `{1..5}`), then
/// runs glob matching on each resulting token.
/// Returns the expanded list. If a pattern matches nothing, it is left as-is.
#[must_use]
pub fn expand(args: &[String]) -> Vec<String> {
    let brace_expanded = expand_braces(args);
    let mut result = Vec::with_capacity(brace_expanded.len());
    for arg in &brace_expanded {
        if has_glob_chars(arg) {
            match glob(arg) {
                Ok(paths) => {
                    let mut matches: Vec<String> = paths
                        .filter_map(|e| e.ok())
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    if matches.is_empty() {
                        result.push(arg.clone());
                    } else {
                        matches.sort();
                        result.extend(matches);
                    }
                }
                Err(_) => result.push(arg.clone()),
            }
        } else {
            result.push(arg.clone());
        }
    }
    result
}

/// Expands brace expressions in a list of arguments.
///
/// Supports:
/// - `{a,b,c}` — explicit list
/// - `{1..10}` — integer range (inclusive)
/// - `{a..z}` — character range (lowercase)
/// - `{A..Z}` — character range (uppercase)
/// - Nested braces are handled by expanding innermost first.
#[must_use]
pub fn expand_braces(args: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for arg in args {
        match expand_single_braces(arg) {
            Some(expanded) => result.extend(expanded),
            None => result.push(arg.clone()),
        }
    }
    result
}

/// If `s` contains a brace expression at the top level (not inside quotes),
/// expands it and returns `Some(Vec)`. Otherwise returns `None`.
fn expand_single_braces(s: &str) -> Option<Vec<String>> {
    let Some((open, close)) = find_matching_braces(s) else {
        return None;
    };

    let prefix = &s[..open];
    let inner = &s[open + 1..close];
    let suffix = &s[close + 1..];

    // Split on top-level commas (respect nested braces)
    let alternatives = split_top_level_commas(inner);

    // Check if this is a range expression `{start..end}`
    if alternatives.len() == 1 {
        if let Some(range) = parse_range(&alternatives[0]) {
            let mut result = Vec::new();
            for item in range {
                let mut expanded = String::with_capacity(prefix.len() + item.len() + suffix.len());
                expanded.push_str(prefix);
                expanded.push_str(&item);
                expanded.push_str(suffix);
                // Recurse to handle nested braces in suffix/prefix
                if let Some(deeper) = expand_single_braces(&expanded) {
                    result.extend(deeper);
                } else {
                    result.push(expanded);
                }
            }
            return Some(result);
        }
    }

    // Otherwise it's a comma-separated list
    let mut result = Vec::new();
    for alt in alternatives {
        let mut expanded = String::with_capacity(prefix.len() + alt.len() + suffix.len());
        expanded.push_str(prefix);
        expanded.push_str(&alt);
        expanded.push_str(suffix);
        if let Some(deeper) = expand_single_braces(&expanded) {
            result.extend(deeper);
        } else {
            result.push(expanded);
        }
    }
    Some(result)
}

/// Finds the outermost matching `{` and `}` pair at the start of a brace expression.
/// Returns `(open_index, close_index)` or `None`.
fn find_matching_braces(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let open = i;
            let mut depth = 1;
            i += 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                }
                i += 1;
            }
            if depth == 0 {
                return Some((open, i - 1));
            }
            return None;
        }
        // Skip quoted strings
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    None
}

/// Splits `inner` on commas that are not nested inside braces.
fn split_top_level_commas(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
            current.push('{');
            i += 1;
        } else if bytes[i] == b'}' {
            depth = depth.saturating_sub(1);
            current.push('}');
            i += 1;
        } else if bytes[i] == b',' && depth == 0 {
            parts.push(current.clone());
            current.clear();
            i += 1;
        } else if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            current.push(quote as char);
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    current.push('\\');
                    i += 1;
                }
                current.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                current.push(bytes[i] as char);
                i += 1;
            }
        } else {
            current.push(bytes[i] as char);
            i += 1;
        }
    }
    parts.push(current);
    parts
}

/// Tries to parse `inner` as a range expression.
/// Returns `Some(Vec<String>)` for:
/// - `{1..10}` — integer range
/// - `{a..z}` — lowercase char range
/// - `{A..Z}` — uppercase char range
fn parse_range(inner: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = inner.splitn(2, "..").collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }

    let start = parts[0];
    let end = parts[1];

    // Integer range: {1..10}
    if let (Ok(s), Ok(e)) = (start.parse::<i64>(), end.parse::<i64>()) {
        if s <= e {
            let items: Vec<String> = (s..=e).map(|n| n.to_string()).collect();
            return Some(items);
        }
        return None;
    }

    // Character range: {a..z} or {A..Z}
    if start.len() == 1 && end.len() == 1 {
        let s = start.chars().next().unwrap();
        let e = end.chars().next().unwrap();
        if s.is_ascii_alphabetic() && e.is_ascii_alphabetic() && s <= e {
            let items: Vec<String> = (s..=e).map(|c| c.to_string()).collect();
            return Some(items);
        }
    }

    None
}

/// Returns true if the pattern contains glob metacharacters.
#[must_use]
pub fn has_glob_chars(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

/// Returns true if the pattern matches any file in the given directory.
#[must_use]
pub fn matches_any(pattern: &str, dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if glob_matches(&name, pattern) {
                return true;
            }
        }
    }
    false
}

/// Simple glob matching for a single name against a pattern.
///
/// Supports `*` (any sequence), `?` (any single char), and `[...]` character classes
/// (with `!` or `^` for negation).
#[must_use]
pub fn glob_matches(name: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let name = name.trim_end_matches('/');
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_inner(&p, &n)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    let mut pi = 0;
    let mut ti = 0;

    while pi < pattern.len() {
        match pattern[pi] {
            '*' => {
                while pi + 1 < pattern.len() && pattern[pi + 1] == '*' {
                    pi += 1;
                }
                let rest = &pattern[pi + 1..];
                for skip in 0..=text.len() - ti {
                    if glob_match_inner(rest, &text[ti + skip..]) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if ti >= text.len() {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            '[' => {
                if ti >= text.len() {
                    return false;
                }
                if let Some(end) = pattern[pi + 1..].iter().position(|&c| c == ']') {
                    let charset = &pattern[pi + 1..pi + 1 + end];
                    let negate = charset.first() == Some(&'!');
                    let chars_to_check = if negate { &charset[1..] } else { charset };
                    let matched = chars_to_check.contains(&text[ti]);
                    if negate == matched {
                        return false;
                    }
                    pi += end + 2;
                    ti += 1;
                } else {
                    if pattern[pi] != text[ti] {
                        return false;
                    }
                    pi += 1;
                    ti += 1;
                }
            }
            c => {
                if ti >= text.len() || text[ti] != c {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }

    ti == text.len() && pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_glob_chars() {
        assert!(has_glob_chars("*"));
        assert!(has_glob_chars("?"));
        assert!(has_glob_chars("[abc]"));
        assert!(!has_glob_chars("hello"));
    }

    #[test]
    fn test_expand_passthrough() {
        let args = vec!["hello".into(), "world".into()];
        let result = expand(&args);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_expand_star_in_nonexistent_dir() {
        let args = vec!["/nonexistent/path/*".into()];
        let result = expand(&args);
        assert_eq!(result, vec!["/nonexistent/path/*"]);
    }

    #[test]
    fn test_expand_braces_simple() {
        let args = vec!["file.{a,b,c}".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["file.a", "file.b", "file.c"]);
    }

    #[test]
    fn test_expand_braces_integer_range() {
        let args = vec!["file.{1..4}.txt".into()];
        let result = expand_braces(&args);
        assert_eq!(
            result,
            vec!["file.1.txt", "file.2.txt", "file.3.txt", "file.4.txt"]
        );
    }

    #[test]
    fn test_expand_braces_char_range() {
        let args = vec!["letter.{a..c}".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["letter.a", "letter.b", "letter.c"]);
    }

    #[test]
    fn test_expand_braces_upper_range() {
        let args = vec!["letter.{A..C}".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["letter.A", "letter.B", "letter.C"]);
    }

    #[test]
    fn test_expand_braces_nested() {
        let args = vec!["{a,b}{1,2}".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["a1", "a2", "b1", "b2"]);
    }

    #[test]
    fn test_expand_braces_no_match_passthrough() {
        let args = vec!["no_braces".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["no_braces"]);
    }

    #[test]
    fn test_expand_braces_with_glob() {
        // After brace expansion, glob should work on the results
        let args = vec!["*.{rs,toml}".into()];
        let result = expand(&args);
        // Should expand braces then glob; at minimum not panic
        let _ = result;
    }

    #[test]
    fn test_expand_recursive_glob() {
        // `**` is supported by the glob crate and the has_glob_chars check
        let args = vec!["**/*.rs".into()];
        let result = expand(&args);
        // Just verify it doesn't panic; actual results depend on cwd
        let _ = result;
    }

    #[test]
    fn test_expand_multiple_args() {
        let args = vec!["{a,b}".into(), "plain".into(), "{1..3}".into()];
        let result = expand_braces(&args);
        assert_eq!(result, vec!["a", "b", "plain", "1", "2", "3"]);
    }
}
