//! Theme system for `AsterShell`.
//!
//! Provides a `Theme` trait and built-in color themes for syntax highlighting
//! and prompt rendering.

use std::collections::HashMap;
use std::fmt;

/// An RGB color triplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
}

impl Color {
    /// Creates a new RGB color.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Converts to a `#RRGGBB` hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Semantic color slots used across the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorRole {
    /// Default foreground text.
    Foreground,
    /// Default background.
    Background,
    /// Command/keyword color.
    Command,
    /// String literals.
    String,
    /// Variable references (`$var`).
    Variable,
    /// Operators (`|`, `&&`, `||`).
    Operator,
    /// Redirect symbols (`>`, `<`, `>>`).
    Redirect,
    /// File path arguments.
    Path,
    /// Error/status indicator.
    Error,
    /// Success indicator.
    Success,
    /// Comment text (`# ...`).
    Comment,
    /// Prompt user@host segment.
    PromptUser,
    /// Prompt current directory segment.
    PromptDir,
    /// Prompt git branch segment.
    PromptGit,
    /// Prompt symbol color.
    PromptSymbol,
}

/// A color theme providing colors for each semantic role.
pub trait Theme: Send + Sync {
    /// Returns the name of this theme.
    fn name(&self) -> &str;

    /// Returns the color for the given role, or `None` if the role
    /// should use the terminal default.
    fn color(&self, role: ColorRole) -> Option<Color>;
}

fn make_map(pairs: &[(ColorRole, Color)]) -> HashMap<ColorRole, Color> {
    pairs.iter().copied().collect()
}

macro_rules! theme_colors {
    ($($role:ident => ($r:expr, $g:expr, $b:expr)),+ $(,)?) => {{
        use ColorRole::*;
        make_map(&[
            $(($role, Color::new($r, $g, $b)),)+
        ])
    }};
}

// ─── Built-in themes ────────────────────────────────────────────────

/// Default (Monokai-inspired) theme.
pub struct DefaultTheme;

impl Theme for DefaultTheme {
    fn name(&self) -> &str {
        "default"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (248, 248, 242),
            Background => (39, 40, 34),
            Command    => (249, 38, 114),
            String     => (230, 219, 116),
            Variable   => (102, 217, 239),
            Operator   => (249, 38, 89),
            Redirect   => (166, 226, 46),
            Path       => (166, 226, 46),
            Error      => (249, 38, 114),
            Success    => (166, 226, 46),
            Comment    => (117, 113, 94),
            PromptUser => (102, 217, 239),
            PromptDir  => (166, 226, 46),
            PromptGit  => (249, 38, 114),
            PromptSymbol => (249, 38, 114),
        };
        colors.get(&role).copied()
    }
}

/// Nord theme.
pub struct NordTheme;

impl Theme for NordTheme {
    fn name(&self) -> &str {
        "nord"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (236, 239, 244),
            Background => (46, 52, 64),
            Command    => (136, 192, 208),
            String     => (163, 190, 140),
            Variable   => (180, 190, 254),
            Operator   => (191, 97, 106),
            Redirect   => (191, 97, 106),
            Path       => (163, 190, 140),
            Error      => (191, 97, 106),
            Success    => (163, 190, 140),
            Comment    => (129, 137, 153),
            PromptUser => (136, 192, 208),
            PromptDir  => (163, 190, 140),
            PromptGit  => (208, 135, 112),
            PromptSymbol => (136, 192, 208),
        };
        colors.get(&role).copied()
    }
}

/// Catppuccin Mocha theme.
pub struct CatppuccinTheme;

impl Theme for CatppuccinTheme {
    fn name(&self) -> &str {
        "catppuccin"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (205, 214, 244),
            Background => (30, 30, 46),
            Command    => (137, 180, 250),
            String     => (166, 227, 161),
            Variable   => (245, 224, 220),
            Operator   => (243, 139, 168),
            Redirect   => (250, 179, 135),
            Path       => (166, 227, 161),
            Error      => (243, 139, 168),
            Success    => (166, 227, 161),
            Comment    => (108, 112, 134),
            PromptUser => (137, 180, 250),
            PromptDir  => (166, 227, 161),
            PromptGit  => (250, 179, 135),
            PromptSymbol => (203, 166, 247),
        };
        colors.get(&role).copied()
    }
}

/// Tokyo Night theme.
pub struct TokyoNightTheme;

impl Theme for TokyoNightTheme {
    fn name(&self) -> &str {
        "tokyonight"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (192, 202, 245),
            Background => (26, 27, 38),
            Command    => (125, 207, 255),
            String     => (158, 206, 106),
            Variable   => (192, 202, 245),
            Operator   => (247, 118, 142),
            Redirect   => (255, 158, 100),
            Path       => (158, 206, 106),
            Error      => (247, 118, 142),
            Success    => (158, 206, 106),
            Comment    => (115, 118, 141),
            PromptUser => (125, 207, 255),
            PromptDir  => (158, 206, 106),
            PromptGit  => (255, 158, 100),
            PromptSymbol => (192, 202, 245),
        };
        colors.get(&role).copied()
    }
}

/// Gruvbox theme.
pub struct GruvboxTheme;

impl Theme for GruvboxTheme {
    fn name(&self) -> &str {
        "gruvbox"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (235, 219, 178),
            Background => (40, 40, 40),
            Command    => (131, 165, 152),
            String     => (184, 187, 38),
            Variable   => (214, 153, 104),
            Operator   => (214, 94, 98),
            Redirect   => (214, 94, 98),
            Path       => (184, 187, 38),
            Error      => (214, 94, 98),
            Success    => (184, 187, 38),
            Comment    => (124, 111, 100),
            PromptUser => (131, 165, 152),
            PromptDir  => (184, 187, 38),
            PromptGit  => (214, 153, 104),
            PromptSymbol => (214, 94, 98),
        };
        colors.get(&role).copied()
    }
}

/// Solarized Dark theme.
pub struct SolarizedTheme;

impl Theme for SolarizedTheme {
    fn name(&self) -> &str {
        "solarized"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (131, 148, 150),
            Background => (0, 43, 54),
            Command    => (38, 139, 210),
            String     => (133, 153, 0),
            Variable   => (211, 54, 130),
            Operator   => (203, 75, 22),
            Redirect   => (203, 75, 22),
            Path       => (133, 153, 0),
            Error      => (220, 50, 47),
            Success    => (133, 153, 0),
            Comment    => (88, 110, 117),
            PromptUser => (38, 139, 210),
            PromptDir  => (133, 153, 0),
            PromptGit  => (211, 54, 130),
            PromptSymbol => (203, 75, 22),
        };
        colors.get(&role).copied()
    }
}

/// Dracula theme.
pub struct DraculaTheme;

impl Theme for DraculaTheme {
    fn name(&self) -> &str {
        "dracula"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (248, 248, 242),
            Background => (40, 42, 54),
            Command    => (255, 121, 198),
            String     => (241, 250, 140),
            Variable   => (189, 147, 249),
            Operator   => (255, 85, 85),
            Redirect   => (255, 85, 85),
            Path       => (80, 250, 123),
            Error      => (255, 85, 85),
            Success    => (80, 250, 123),
            Comment    => (98, 114, 164),
            PromptUser => (189, 147, 249),
            PromptDir  => (80, 250, 123),
            PromptGit  => (255, 121, 198),
            PromptSymbol => (255, 121, 198),
        };
        colors.get(&role).copied()
    }
}

/// One Dark theme.
pub struct OneDarkTheme;

impl Theme for OneDarkTheme {
    fn name(&self) -> &str {
        "onedark"
    }

    fn color(&self, role: ColorRole) -> Option<Color> {
        let colors = theme_colors! {
            Foreground => (171, 178, 191),
            Background => (40, 44, 52),
            Command    => (198, 120, 221),
            String     => (152, 195, 121),
            Variable   => (224, 175, 104),
            Operator   => (190, 80, 70),
            Redirect   => (190, 80, 70),
            Path       => (86, 182, 194),
            Error      => (224, 108, 117),
            Success    => (152, 195, 121),
            Comment    => (92, 99, 112),
            PromptUser => (198, 120, 221),
            PromptDir  => (86, 182, 194),
            PromptGit  => (224, 175, 104),
            PromptSymbol => (198, 120, 221),
        };
        colors.get(&role).copied()
    }
}

/// Returns the built-in theme list.
pub fn builtin_themes() -> Vec<Box<dyn Theme>> {
    vec![
        Box::new(DefaultTheme),
        Box::new(NordTheme),
        Box::new(CatppuccinTheme),
        Box::new(TokyoNightTheme),
        Box::new(GruvboxTheme),
        Box::new(SolarizedTheme),
        Box::new(DraculaTheme),
        Box::new(OneDarkTheme),
    ]
}

/// Looks up a theme by name (case-insensitive).
pub fn find_theme(name: &str) -> Option<Box<dyn Theme>> {
    let lower = name.to_ascii_lowercase();
    builtin_themes().into_iter().find(|t| t.name() == lower)
}

/// Returns the names of all built-in themes.
#[must_use]
pub fn theme_names() -> Vec<&'static str> {
    vec![
        "default",
        "nord",
        "catppuccin",
        "tokyonight",
        "gruvbox",
        "solarized",
        "dracula",
        "onedark",
    ]
}

/// A theme registry for user-defined and built-in themes.
pub struct ThemeRegistry {
    themes: HashMap<String, Box<dyn Theme>>,
}

impl ThemeRegistry {
    /// Creates a new registry pre-populated with built-in themes.
    #[must_use]
    pub fn new() -> Self {
        let mut themes: HashMap<String, Box<dyn Theme>> = HashMap::new();
        for theme in builtin_themes() {
            let name = theme.name().to_ascii_lowercase();
            themes.insert(name, theme);
        }
        Self { themes }
    }

    /// Registers a custom theme.
    pub fn register(&mut self, theme: Box<dyn Theme>) {
        let name = theme.name().to_ascii_lowercase();
        self.themes.insert(name, theme);
    }

    /// Looks up a theme by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Theme> {
        self.themes
            .get(&name.to_ascii_lowercase())
            .map(std::convert::AsRef::as_ref)
    }

    /// Returns all registered theme names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.themes
            .keys()
            .map(std::string::String::as_str)
            .collect()
    }
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_to_hex() {
        let c = Color::new(255, 128, 0);
        assert_eq!(c.to_hex(), "#FF8000");
    }

    #[test]
    fn test_theme_names() {
        let names = theme_names();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"nord"));
        assert!(names.contains(&"catppuccin"));
        assert!(names.contains(&"dracula"));
    }

    #[test]
    fn test_find_theme() {
        let t = find_theme("nord");
        assert!(t.is_some());
        assert_eq!(t.unwrap().name(), "nord");
    }

    #[test]
    fn test_find_theme_case_insensitive() {
        let t = find_theme("NORD");
        assert!(t.is_some());
    }

    #[test]
    fn test_registry() {
        let reg = ThemeRegistry::new();
        assert!(reg.get("default").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_all_themes_have_colors() {
        for theme in builtin_themes() {
            assert!(theme.color(ColorRole::Command).is_some());
            assert!(theme.color(ColorRole::String).is_some());
            assert!(theme.color(ColorRole::Variable).is_some());
        }
    }
}
