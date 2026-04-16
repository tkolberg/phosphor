use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub theme: Option<String>,
    pub ghostty: Option<String>,
}

/// Extract YAML front matter from the beginning of a markdown document.
/// Returns the parsed front matter (if any) and the remaining markdown content.
pub fn extract_front_matter(markdown: &str) -> (Option<FrontMatter>, &str) {
    let trimmed = markdown.trim_start();

    // Front matter must start with "---" on its own line
    if !trimmed.starts_with("---") {
        return (None, markdown);
    }

    let after_opening = &trimmed[3..];
    // Find the closing "---"
    if let Some(end) = after_opening.find("\n---") {
        let yaml_content = &after_opening[..end];
        let rest_start = end + 4; // skip past "\n---"
        let rest = after_opening[rest_start..].trim_start_matches('\n');

        match serde_yaml::from_str::<FrontMatter>(yaml_content) {
            Ok(fm) => (Some(fm), rest),
            Err(_) => (None, markdown),
        }
    } else {
        (None, markdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_front_matter() {
        let md = "---\ntitle: My Talk\nauthor: Ted\ntheme: dark.yaml\n---\n\n# Slide 1\n";
        let (fm, rest) = extract_front_matter(md);
        let fm = fm.unwrap();
        assert_eq!(fm.title.as_deref(), Some("My Talk"));
        assert_eq!(fm.author.as_deref(), Some("Ted"));
        assert_eq!(fm.theme.as_deref(), Some("dark.yaml"));
        assert!(rest.starts_with("# Slide 1"));
    }

    #[test]
    fn test_no_front_matter() {
        let md = "# Just a heading\n\nSome content\n";
        let (fm, rest) = extract_front_matter(md);
        assert!(fm.is_none());
        assert_eq!(rest, md);
    }
}
