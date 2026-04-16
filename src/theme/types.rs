use std::collections::HashMap;

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Theme {
    #[serde(default)]
    pub palette: HashMap<String, String>,
    #[serde(default)]
    pub styles: ThemeStyles,
    #[serde(default)]
    pub footer: Option<FooterTheme>,
    #[serde(default)]
    pub slide: Option<SlideStyle>,
    /// Semantic highlight classes for prose markup: `{class: text}`.
    #[serde(default)]
    pub highlights: HashMap<String, HighlightStyle>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThemeStyles {
    pub heading: Option<ElementStyle>,
    pub paragraph: Option<ElementStyle>,
    pub code: Option<CodeStyle>,
    pub list: Option<ElementStyle>,
    pub blockquote: Option<ElementStyle>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ElementStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CodeStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub padding: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlideStyle {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub margin: Option<MarginConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MarginConfig {
    pub left: Option<u16>,
    pub right: Option<u16>,
    pub top: Option<u16>,
    pub bottom: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FooterTheme {
    pub template: Option<String>,
    pub fg: Option<String>,
    pub bg: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HighlightStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

impl Theme {
    /// Resolve a color string to a ratatui Color.
    /// Supports: hex (#rrggbb), palette references (palette:name), and named colors.
    pub fn resolve_color(&self, color_str: &str) -> Option<Color> {
        resolve_color_with_palette(color_str, &self.palette)
    }
}

pub fn resolve_color_with_palette(
    color_str: &str,
    palette: &HashMap<String, String>,
) -> Option<Color> {
    // Palette reference
    if let Some(name) = color_str.strip_prefix("palette:") {
        let resolved = palette.get(name)?;
        return resolve_color_with_palette(resolved, palette);
    }

    // Hex color
    if let Some(hex) = color_str.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        return None;
    }

    // Named colors
    match color_str.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_color() {
        let theme = Theme::default();
        assert_eq!(
            theme.resolve_color("#ff8800"),
            Some(Color::Rgb(255, 136, 0))
        );
    }

    #[test]
    fn test_named_color() {
        let theme = Theme::default();
        assert_eq!(theme.resolve_color("cyan"), Some(Color::Cyan));
    }

    #[test]
    fn test_palette_reference() {
        let mut theme = Theme::default();
        theme
            .palette
            .insert("primary".to_string(), "#00aaff".to_string());
        assert_eq!(
            theme.resolve_color("palette:primary"),
            Some(Color::Rgb(0, 170, 255))
        );
    }
}
