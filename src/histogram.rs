use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use ratatui::style::Color;
use ratatui::text::{Line, Span};
use serde::Deserialize;

static HISTOGRAM_CACHE: std::sync::LazyLock<Mutex<HashMap<(String, String, String, u16, u16), Vec<Line<'static>>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Deserialize)]
pub struct HistogramSpec {
    pub file: String,
    pub variable: String,
    pub category: String,
    #[serde(default)]
    pub signal_scale: Option<f64>,
    #[serde(default)]
    pub log: Option<bool>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistogramFile {
    signal_names: Vec<String>,
    histograms: Vec<HistogramEntry>,
}

#[derive(Debug, Deserialize)]
struct HistogramEntry {
    variable: String,
    category: String,
    bin_edges: Vec<f64>,
    xlabel: Option<String>,
    ylabel: Option<String>,
    processes: HashMap<String, ProcessData>,
}

#[derive(Debug, Deserialize)]
struct ProcessData {
    role: String,
    values: Vec<f64>,
    #[allow(dead_code)]
    variances: Option<Vec<f64>>,
    label: Option<String>,
}

pub fn parse_histogram_spec(yaml: &str) -> Option<HistogramSpec> {
    serde_yaml::from_str(yaml).ok()
}

const BG_COLORS: &[Color] = &[
    Color::Rgb(70, 130, 180),  // steel blue — QCD
    Color::Rgb(178, 102, 44),  // rust — ttbar-hadronic
    Color::Rgb(210, 150, 60),  // amber — ttbar-semilep
    Color::Rgb(180, 120, 80),  // tan — ttbar-dilep
    Color::Rgb(130, 170, 100), // sage — single-t
    Color::Rgb(160, 100, 160), // mauve — ttX
    Color::Rgb(100, 180, 140), // teal — W+jets
    Color::Rgb(80, 140, 170),  // slate — Z+jets
    Color::Rgb(170, 130, 180), // lavender — VV
    Color::Rgb(140, 160, 100), // olive — VH
    Color::Rgb(180, 140, 140), // rose — VVV
    Color::Rgb(130, 130, 150), // gray — Other
];

const SIGNAL_COLORS: &[Color] = &[
    Color::Rgb(255, 41, 117),  // hot pink
    Color::Rgb(0, 229, 255),   // neon cyan
    Color::Rgb(255, 211, 25),  // warm yellow
];

pub fn render_histogram(
    spec: &HistogramSpec,
    base_dir: &Path,
    term_cols: u16,
    term_rows: u16,
) -> Vec<Line<'static>> {
    let cache_key = (
        spec.variable.clone(),
        spec.category.clone(),
        spec.file.clone(),
        term_cols,
        term_rows,
    );
    if let Ok(cache) = HISTOGRAM_CACHE.lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = render_histogram_inner(spec, base_dir, term_cols, term_rows);

    if let Ok(mut cache) = HISTOGRAM_CACHE.lock() {
        cache.insert(cache_key, result.clone());
    }

    result
}

fn render_histogram_inner(
    spec: &HistogramSpec,
    base_dir: &Path,
    term_cols: u16,
    term_rows: u16,
) -> Vec<Line<'static>> {
    let path = base_dir.join(&spec.file);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![Line::raw(format!("[histogram: cannot read {:?}]", path))],
    };
    let file: HistogramFile = match serde_json::from_str(&content) {
        Ok(f) => f,
        Err(e) => return vec![Line::raw(format!("[histogram: parse error: {}]", e))],
    };

    let entry = match file
        .histograms
        .iter()
        .find(|h| h.variable == spec.variable && h.category == spec.category)
    {
        Some(e) => e,
        None => return vec![Line::raw("[histogram: variable/category not found]")],
    };

    let use_log = spec.log.unwrap_or(false);
    let signal_scale = spec.signal_scale.unwrap_or(1.0);
    let n_bins = entry.bin_edges.len().saturating_sub(1);
    if n_bins == 0 {
        return vec![Line::raw("[histogram: no bins]")];
    }

    // Separate processes by role
    let mut backgrounds: Vec<(String, &[f64], Color)> = Vec::new();
    let mut signals: Vec<(String, &[f64], Color, String)> = Vec::new();
    let mut data_values: Option<&[f64]> = None;

    let mut bg_idx = 0;
    let mut sig_idx = 0;
    for (name, proc) in &entry.processes {
        match proc.role.as_str() {
            "background" => {
                let color = BG_COLORS[bg_idx % BG_COLORS.len()];
                let label = clean_latex(proc.label.as_deref().unwrap_or(name));
                backgrounds.push((label, &proc.values, color));
                bg_idx += 1;
            }
            "signal" => {
                let color = SIGNAL_COLORS[sig_idx % SIGNAL_COLORS.len()];
                let label = clean_latex(proc.label.as_deref().unwrap_or(name));
                let display_label = if signal_scale != 1.0 {
                    format!("{} (x{:.0})", label, signal_scale)
                } else {
                    label.clone()
                };
                signals.push((label, &proc.values, color, display_label));
                sig_idx += 1;
            }
            "data" => {
                data_values = Some(&proc.values);
            }
            _ => {}
        }
    }

    // Sort backgrounds by total yield (smallest first = bottom of stack, visible on log scale)
    backgrounds.sort_by(|a, b| {
        let sum_a: f64 = a.1.iter().sum();
        let sum_b: f64 = b.1.iter().sum();
        sum_a.partial_cmp(&sum_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute stacked totals per bin
    let mut stack: Vec<Vec<f64>> = Vec::new(); // stack[layer][bin] = cumulative top
    let mut cumulative = vec![0.0f64; n_bins];
    for (_, values, _) in &backgrounds {
        for (i, &v) in values.iter().enumerate().take(n_bins) {
            cumulative[i] += v.max(0.0);
        }
        stack.push(cumulative.clone());
    }

    // Find y-range
    let mut y_max = cumulative.iter().cloned().fold(0.0f64, f64::max);

    // Include scaled signal in y_max
    for (_, values, _, _) in &signals {
        for (i, &v) in values.iter().enumerate().take(n_bins) {
            let sv = v * signal_scale;
            if sv > y_max {
                y_max = sv;
            }
        }
    }

    // Include data in y_max
    if let Some(dv) = data_values {
        for &v in dv.iter().take(n_bins) {
            if v > y_max {
                y_max = v;
            }
        }
    }

    if y_max <= 0.0 {
        y_max = 1.0;
    }
    y_max *= 1.1; // 10% headroom

    // Layout: y-axis labels on left, plot area in halfblock
    let y_label_width: u16 = 9;
    let plot_cols = term_cols.saturating_sub(y_label_width) as usize;
    let plot_rows = term_rows.saturating_sub(3) as usize; // room for x-axis + legend

    if plot_cols < 10 || plot_rows < 4 {
        return vec![Line::raw("[histogram: too small]")];
    }

    // Halfblock: 2 vertical pixels per terminal row
    let px_w = plot_cols;
    let px_h = plot_rows * 2;

    // Pixel grid: None = transparent (use slide bg), Some(color) = filled
    let mut pixels: Vec<Vec<Option<Color>>> = vec![vec![None; px_w]; px_h];

    let val_to_py = |v: f64| -> usize {
        let frac = if use_log {
            let log_min = 0.1f64.log10();
            let log_max = y_max.log10();
            let log_v = v.max(0.1).log10();
            (log_v - log_min) / (log_max - log_min)
        } else {
            v / y_max
        };
        let py = (1.0 - frac.clamp(0.0, 1.0)) * (px_h as f64 - 1.0);
        (py as usize).min(px_h - 1)
    };

    let bin_to_col = |bin: usize| -> (usize, usize) {
        let x0 = bin * px_w / n_bins;
        let x1 = (bin + 1) * px_w / n_bins;
        (x0, x1)
    };

    // Draw stacked backgrounds
    for (layer_idx, ((_, _, color), top_values)) in
        backgrounds.iter().zip(stack.iter()).enumerate()
    {
        let bottom_values = if layer_idx > 0 {
            &stack[layer_idx - 1]
        } else {
            &vec![0.0; n_bins]
        };

        for bin in 0..n_bins {
            let (col0, col1) = bin_to_col(bin);
            let py_top = val_to_py(top_values[bin]);
            let py_bot = val_to_py(bottom_values[bin]);

            for x in col0..col1 {
                for y in py_top..=py_bot {
                    if x < px_w && y < px_h {
                        pixels[y][x] = Some(*color);
                    }
                }
            }
        }
    }

    // Draw signal as step function (overwrites pixels on the outline)
    for (_, values, color, _) in &signals {
        for bin in 0..n_bins {
            let (col0, col1) = bin_to_col(bin);
            let sv = values[bin] * signal_scale;
            let py = val_to_py(sv);
            // Horizontal top edge
            for x in col0..col1 {
                if x < px_w && py < px_h {
                    pixels[py][x] = Some(*color);
                }
            }
            // Vertical left edge connecting to previous bin
            if bin > 0 {
                let prev_sv = values[bin - 1] * signal_scale;
                let prev_py = val_to_py(prev_sv);
                let (y0, y1) = if py < prev_py { (py, prev_py) } else { (prev_py, py) };
                for y in y0..=y1 {
                    if col0 < px_w && y < px_h {
                        pixels[y][col0] = Some(*color);
                    }
                }
            }
        }
    }

    // Draw data as single-pixel dots at bin centers
    if let Some(dv) = data_values {
        let data_color = Color::White;
        for bin in 0..n_bins {
            let (col0, col1) = bin_to_col(bin);
            let cx = (col0 + col1) / 2;
            let py = val_to_py(dv[bin]);
            if cx < px_w && py < px_h {
                pixels[py][cx] = Some(data_color);
            }
        }
    }

    // Render pixel grid to halfblock lines
    // ▀ (U+2580): top half block. fg = top pixel, bg = bottom pixel.
    let mut output: Vec<Line<'static>> = Vec::new();
    let n_y_ticks = 5usize;
    let y_label_w = y_label_width as usize;

    for row in 0..plot_rows {
        let top_y = row * 2;
        let bot_y = row * 2 + 1;

        let mut spans: Vec<Span<'static>> = Vec::new();

        // Y-axis label
        let frac = row as f64 / (plot_rows - 1).max(1) as f64;
        let tick_row = (frac * (n_y_ticks - 1) as f64).round() as usize;
        let expected_row =
            (tick_row as f64 / (n_y_ticks - 1) as f64 * (plot_rows - 1) as f64).round() as usize;

        if row == expected_row {
            let val = if use_log {
                let log_min = 0.1f64.log10();
                let log_max = y_max.log10();
                let log_v = log_max - frac * (log_max - log_min);
                10.0f64.powf(log_v)
            } else {
                y_max * (1.0 - frac)
            };
            let label = format_value(val);
            let padded = format!("{:>width$} ", label, width = y_label_w - 2);
            spans.push(Span::styled(
                padded,
                ratatui::style::Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(y_label_w)));
        }

        // Build halfblock spans, grouping consecutive columns with the same color pair
        let mut col = 0;
        while col < plot_cols {
            let top = pixels[top_y][col];
            let bot = if bot_y < px_h { pixels[bot_y][col] } else { None };

            // Find run of identical color pairs
            let mut run_end = col + 1;
            while run_end < plot_cols {
                let t = pixels[top_y][run_end];
                let b = if bot_y < px_h { pixels[bot_y][run_end] } else { None };
                if t != top || b != bot {
                    break;
                }
                run_end += 1;
            }
            let run_len = run_end - col;

            match (top, bot) {
                (None, None) => {
                    spans.push(Span::raw(" ".repeat(run_len)));
                }
                (Some(tc), Some(bc)) if tc == bc => {
                    spans.push(Span::styled(
                        "█".repeat(run_len),
                        ratatui::style::Style::default().fg(tc),
                    ));
                }
                (Some(tc), Some(bc)) => {
                    spans.push(Span::styled(
                        "▀".repeat(run_len),
                        ratatui::style::Style::default().fg(tc).bg(bc),
                    ));
                }
                (Some(tc), None) => {
                    spans.push(Span::styled(
                        "▀".repeat(run_len),
                        ratatui::style::Style::default().fg(tc),
                    ));
                }
                (None, Some(bc)) => {
                    spans.push(Span::styled(
                        "▄".repeat(run_len),
                        ratatui::style::Style::default().fg(bc),
                    ));
                }
            }

            col = run_end;
        }

        output.push(Line::from(spans));
    }

    // X-axis labels
    let x_min = entry.bin_edges[0];
    let x_max = entry.bin_edges[n_bins];
    let padding = " ".repeat(y_label_w);
    let plot_w = plot_cols as usize;

    let xlabel_str = entry.xlabel.as_deref().unwrap_or(&spec.variable);
    // Strip any LaTeX-like formatting for terminal display
    let xlabel_clean = xlabel_str
        .replace("$", "")
        .replace("\\", "")
        .replace("{", "")
        .replace("}", "");

    let fmt_axis = |v: f64| -> String {
        if v.fract().abs() < 1e-9 {
            format!("{:.0}", v)
        } else {
            format!("{}", v)
        }
    };
    let x_left = fmt_axis(x_min);
    let x_right = fmt_axis(x_max);
    let x_mid_val = (x_min + x_max) / 2.0;
    let x_mid = fmt_axis(x_mid_val);

    let mid_pos = plot_w / 2;
    let right_pos = plot_w.saturating_sub(x_right.len());
    let mut x_axis_chars: Vec<u8> = vec![b' '; plot_w];
    for (i, b) in x_left.bytes().enumerate() {
        if i < plot_w { x_axis_chars[i] = b; }
    }
    for (i, b) in x_mid.bytes().enumerate() {
        let pos = mid_pos.saturating_sub(x_mid.len() / 2) + i;
        if pos < plot_w { x_axis_chars[pos] = b; }
    }
    for (i, b) in x_right.bytes().enumerate() {
        let pos = right_pos + i;
        if pos < plot_w { x_axis_chars[pos] = b; }
    }

    output.push(Line::from(vec![
        Span::raw(padding.clone()),
        Span::styled(
            String::from_utf8_lossy(&x_axis_chars).to_string(),
            ratatui::style::Style::default().fg(Color::DarkGray),
        ),
    ]));

    // X-axis label (centered)
    let xlabel_pad = (plot_w + y_label_w).saturating_sub(xlabel_clean.len()) / 2;
    output.push(Line::from(vec![Span::styled(
        format!("{:>width$}", xlabel_clean, width = xlabel_pad + xlabel_clean.len()),
        ratatui::style::Style::default().fg(Color::Gray),
    )]));

    // Legend line(s) — compact, all on one or two lines
    let mut legend_spans: Vec<Span<'static>> = Vec::new();
    legend_spans.push(Span::raw(" ".repeat(y_label_w)));

    // Data
    if data_values.is_some() {
        legend_spans.push(Span::styled("● ", ratatui::style::Style::default().fg(Color::White)));
        legend_spans.push(Span::styled("Data  ", ratatui::style::Style::default().fg(Color::DarkGray)));
    }

    // Backgrounds (show top few)
    let max_bg_legend = 6;
    for (i, (label, _, color)) in backgrounds.iter().enumerate().take(max_bg_legend) {
        legend_spans.push(Span::styled("█ ", ratatui::style::Style::default().fg(*color)));
        let display = if label.len() > 10 {
            format!("{}  ", &label[..10])
        } else {
            format!("{}  ", label)
        };
        legend_spans.push(Span::styled(display, ratatui::style::Style::default().fg(Color::DarkGray)));
    }
    if backgrounds.len() > max_bg_legend {
        legend_spans.push(Span::styled(
            format!("+{}  ", backgrounds.len() - max_bg_legend),
            ratatui::style::Style::default().fg(Color::DarkGray),
        ));
    }

    output.push(Line::from(legend_spans));

    // Signal legend on next line
    if !signals.is_empty() {
        let mut sig_spans: Vec<Span<'static>> = Vec::new();
        sig_spans.push(Span::raw(" ".repeat(y_label_w)));
        for (_, _, color, display_label) in &signals {
            sig_spans.push(Span::styled("━ ", ratatui::style::Style::default().fg(*color)));
            sig_spans.push(Span::styled(
                format!("{}  ", display_label),
                ratatui::style::Style::default().fg(Color::DarkGray),
            ));
        }
        output.push(Line::from(sig_spans));
    }

    output
}

fn clean_latex(s: &str) -> String {
    s.replace("$", "")
        .replace("\\kappa_", "k")
        .replace("\\lambda", "L")
        .replace("\\", "")
        .replace("{", "")
        .replace("}", "")
}

fn format_value(v: f64) -> String {
    if v.abs() >= 1e6 {
        format!("{:.1e}", v)
    } else if v.abs() >= 1000.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 1.0 {
        format!("{:.1}", v)
    } else if v.abs() >= 0.01 {
        format!("{:.2}", v)
    } else {
        format!("{:.1e}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spec() {
        let yaml = "file: histos.json\nvariable: met_pt\ncategory: cat1\nlog: true\nsignal_scale: 1000";
        let spec = parse_histogram_spec(yaml).unwrap();
        assert_eq!(spec.file, "histos.json");
        assert_eq!(spec.variable, "met_pt");
        assert_eq!(spec.log, Some(true));
        assert_eq!(spec.signal_scale, Some(1000.0));
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(1.2e7), "1.2e7");
        assert_eq!(format_value(5000.0), "5000");
        assert_eq!(format_value(3.5), "3.5");
        assert_eq!(format_value(0.05), "0.05");
    }
}
