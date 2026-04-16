use ratatui::style::Color;

use crate::chart::{ChartData, ChartSpec};
use crate::elements::StyledText;

#[derive(Debug, Clone)]
pub enum RenderOp {
    /// Clear the current window area
    ClearRect,
    /// Move cursor to a specific row within the current window
    JumpToRow { row: u16 },
    /// Render a line of styled text
    RenderText {
        line: StyledText,
        alignment: Alignment,
    },
    /// Insert blank lines
    Spacer { lines: u16 },
    /// Push a narrower window rect (apply margins)
    PushWindowRect { margin: Margin },
    /// Pop back to the parent window rect
    PopWindowRect,
    /// Set default colors for subsequent operations
    SetColors {
        fg: Option<Color>,
        bg: Option<Color>,
    },
    /// Render a chart widget
    RenderChart {
        spec: ChartSpec,
        data: ChartData,
        height: u16,
    },
    /// Render an image using halfblock characters
    RenderImage {
        lines: Vec<ratatui::text::Line<'static>>,
        width: u16,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Margin {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}
