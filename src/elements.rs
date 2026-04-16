use std::path::PathBuf;

use serde::Serialize;

use crate::chart::ChartSpec;

#[derive(Debug, Clone, Serialize)]
pub enum SlideElement {
    Heading {
        level: u8,
        text: StyledText,
    },
    Paragraph {
        text: StyledText,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    List {
        items: Vec<ListItem>,
        ordered: bool,
        start: usize,
    },
    BlockQuote {
        text: StyledText,
    },
    Chart {
        #[serde(skip)]
        spec: ChartSpec,
        #[serde(skip)]
        base_dir: PathBuf,
    },
    Diagram {
        source: String,
    },
    Table {
        headers: Vec<StyledText>,
        rows: Vec<Vec<StyledText>>,
        alignments: Vec<TableAlignment>,
    },
    Image {
        path: String,
        alt: String,
        #[serde(skip)]
        base_dir: PathBuf,
    },
    Wireframe {
        source: String,
    },
    HorizontalRule,
    Spacer,
    /// Internal marker for chunk boundaries (not rendered)
    ChunkBreak,
}

#[derive(Debug, Clone, Serialize)]
pub struct StyledText {
    pub segments: Vec<TextSegment>,
}

impl StyledText {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            segments: vec![TextSegment {
                text: text.into(),
                style: SegmentStyle::default(),
            }],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.text.is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextSegment {
    pub text: String,
    pub style: SegmentStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SegmentStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    /// Semantic highlight class (e.g. "key", "jargon", "definition").
    /// Resolved to colors via the theme's `highlights` map.
    pub highlight: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListItem {
    pub depth: u8,
    pub text: StyledText,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}
