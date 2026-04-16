use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Bit offsets for each pixel within a 2×4 Braille cell.
/// Layout:
///   (0,0)=0  (1,0)=3
///   (0,1)=1  (1,1)=4
///   (0,2)=2  (1,2)=5
///   (0,3)=6  (1,3)=7
const BRAILLE_MAP: [[u8; 4]; 2] = [
    [0, 1, 2, 6], // col 0
    [3, 4, 5, 7], // col 1
];

/// A pixel-addressable canvas that renders to Unicode Braille characters.
/// Each terminal cell is a 2×4 dot grid, giving 2x horizontal and 4x vertical
/// sub-cell resolution.
pub struct BrailleCanvas {
    /// Pixel buffer: true = dot on
    pixels: Vec<bool>,
    /// Per-cell foreground color (indexed by cell_y * cell_cols + cell_x)
    colors: Vec<Option<Color>>,
    /// Pixel dimensions
    pub width: usize,
    pub height: usize,
    /// Cell dimensions
    cell_cols: usize,
    cell_rows: usize,
}

impl BrailleCanvas {
    /// Create a canvas sized to fill `term_cols` × `term_rows` terminal cells.
    /// Pixel resolution is (term_cols * 2) × (term_rows * 4).
    pub fn new(term_cols: u16, term_rows: u16) -> Self {
        let cell_cols = term_cols as usize;
        let cell_rows = term_rows as usize;
        let width = cell_cols * 2;
        let height = cell_rows * 4;
        Self {
            pixels: vec![false; width * height],
            colors: vec![None; cell_cols * cell_rows],
            width,
            height,
            cell_cols,
            cell_rows,
        }
    }

    /// Set a single pixel.
    pub fn set(&mut self, x: isize, y: isize) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let (xu, yu) = (x as usize, y as usize);
            self.pixels[yu * self.width + xu] = true;
        }
    }

    /// Set a pixel with a color (applied to the containing cell).
    pub fn set_colored(&mut self, x: isize, y: isize, color: Color) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let (xu, yu) = (x as usize, y as usize);
            self.pixels[yu * self.width + xu] = true;
            let cx = xu / 2;
            let cy = yu / 4;
            self.colors[cy * self.cell_cols + cx] = Some(color);
        }
    }

    /// Clear a pixel.
    pub fn clear(&mut self, x: isize, y: isize) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.pixels[y as usize * self.width + x as usize] = false;
        }
    }

    /// Draw a line between two points (Bresenham's algorithm).
    pub fn line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx: isize = if x0 < x1 { 1 } else { -1 };
        let sy: isize = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            self.set(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                if x == x1 {
                    break;
                }
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                if y == y1 {
                    break;
                }
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a line with a specific color.
    pub fn line_colored(&mut self, x0: isize, y0: isize, x1: isize, y1: isize, color: Color) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx: isize = if x0 < x1 { 1 } else { -1 };
        let sy: isize = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            self.set_colored(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                if x == x1 {
                    break;
                }
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                if y == y1 {
                    break;
                }
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a closed polygon from a list of vertices.
    pub fn polygon(&mut self, points: &[(isize, isize)]) {
        if points.len() < 2 {
            return;
        }
        for i in 0..points.len() {
            let j = (i + 1) % points.len();
            self.line(points[i].0, points[i].1, points[j].0, points[j].1);
        }
    }

    /// Draw a closed polygon with a specific color.
    pub fn polygon_colored(&mut self, points: &[(isize, isize)], color: Color) {
        if points.len() < 2 {
            return;
        }
        for i in 0..points.len() {
            let j = (i + 1) % points.len();
            self.line_colored(points[i].0, points[i].1, points[j].0, points[j].1, color);
        }
    }

    /// Draw a circle (midpoint algorithm).
    pub fn circle(&mut self, cx: isize, cy: isize, r: isize) {
        let mut x = r;
        let mut y: isize = 0;
        let mut err = 1 - r;

        while x >= y {
            self.set(cx + x, cy + y);
            self.set(cx - x, cy + y);
            self.set(cx + x, cy - y);
            self.set(cx - x, cy - y);
            self.set(cx + y, cy + x);
            self.set(cx - y, cy + x);
            self.set(cx + y, cy - x);
            self.set(cx - y, cy - x);

            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }

    /// Render the canvas to ratatui Lines.
    pub fn render(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(self.cell_rows);

        for cy in 0..self.cell_rows {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut current_color: Option<Color> = None;
            let mut current_chars = String::new();

            for cx in 0..self.cell_cols {
                let mut code: u8 = 0;
                for col in 0..2usize {
                    for row in 0..4usize {
                        let px = cx * 2 + col;
                        let py = cy * 4 + row;
                        if px < self.width && py < self.height && self.pixels[py * self.width + px]
                        {
                            code |= 1 << BRAILLE_MAP[col][row];
                        }
                    }
                }

                let ch = char::from_u32(0x2800 + code as u32).unwrap_or(' ');
                let cell_color = self.colors[cy * self.cell_cols + cx];

                if cell_color != current_color && !current_chars.is_empty() {
                    let style = match current_color {
                        Some(c) => Style::default().fg(c),
                        None => Style::default(),
                    };
                    spans.push(Span::styled(current_chars.clone(), style));
                    current_chars.clear();
                }
                current_color = cell_color;
                current_chars.push(ch);
            }

            if !current_chars.is_empty() {
                let style = match current_color {
                    Some(c) => Style::default().fg(c),
                    None => Style::default(),
                };
                spans.push(Span::styled(current_chars, style));
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pixel() {
        let mut canvas = BrailleCanvas::new(1, 1);
        // Pixel (0,0) → bit 0 → U+2801
        canvas.set(0, 0);
        let lines = canvas.render();
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "\u{2801}");
    }

    #[test]
    fn test_full_cell() {
        let mut canvas = BrailleCanvas::new(1, 1);
        for x in 0..2 {
            for y in 0..4 {
                canvas.set(x, y);
            }
        }
        let lines = canvas.render();
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        // All 8 bits set → U+28FF
        assert_eq!(text, "\u{28FF}");
    }

    #[test]
    fn test_line_horizontal() {
        let mut canvas = BrailleCanvas::new(5, 1);
        canvas.line(0, 0, 9, 0);
        let lines = canvas.render();
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        // Every cell should have top-left and top-right dots lit
        for ch in text.chars() {
            assert_ne!(ch, '\u{2800}', "expected non-empty braille");
        }
    }

    #[test]
    fn test_dimensions() {
        let canvas = BrailleCanvas::new(40, 10);
        assert_eq!(canvas.width, 80);
        assert_eq!(canvas.height, 40);
    }

    #[test]
    fn test_out_of_bounds_no_panic() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set(-1, -1);
        canvas.set(100, 100);
        canvas.line(-10, -10, 100, 100);
        // Should not panic
    }

    #[test]
    fn test_colored_line() {
        let mut canvas = BrailleCanvas::new(5, 1);
        canvas.line_colored(0, 0, 9, 0, Color::Cyan);
        let lines = canvas.render();
        // All spans should have cyan color
        for span in &lines[0].spans {
            assert_eq!(span.style.fg, Some(Color::Cyan));
        }
    }
}
