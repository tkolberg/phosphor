use std::path::Path;

use color_eyre::eyre::{Result, WrapErr};

use super::types::Theme;

pub fn load_theme(path: &Path) -> Result<Theme> {
    let content =
        std::fs::read_to_string(path).wrap_err_with(|| format!("Failed to read theme {:?}", path))?;
    let theme: Theme =
        serde_yaml::from_str(&content).wrap_err_with(|| format!("Failed to parse theme {:?}", path))?;
    Ok(theme)
}

/// Returns a sensible built-in default theme.
pub fn default_theme() -> Theme {
    let yaml = r##"
palette:
  bg: "#1e1e2e"
  fg: "#cdd6f4"
  heading: "#89b4fa"
  accent: "#f5c2e7"
  code_bg: "#313244"
  muted: "#6c7086"

slide:
  bg: "palette:bg"
  fg: "palette:fg"
  margin:
    left: 4
    right: 4
    top: 2
    bottom: 1

styles:
  heading:
    fg: "palette:heading"
    bold: true
  code:
    fg: "palette:fg"
    bg: "palette:code_bg"
    padding: 2
  blockquote:
    fg: "palette:accent"
    italic: true

footer:
  fg: "palette:muted"
"##;
    serde_yaml::from_str(yaml).expect("built-in theme should be valid")
}
