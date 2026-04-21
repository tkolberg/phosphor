use std::path::Path;
use std::process::Command;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde::Deserialize;


#[derive(Debug, Clone, Deserialize)]
pub struct PlotSpec {
    pub file: String,
    pub title: Option<String>,
}

pub fn parse_plot_spec(yaml: &str) -> Option<PlotSpec> {
    serde_yaml::from_str(yaml).ok()
}

// ── Extracted plot data ─────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ScatterPoint {
    x: f64,
    y: f64,
    color: Color,
}

#[derive(Debug, Clone)]
struct PlotLine {
    points: Vec<(f64, f64)>,
    color: Color,
    dashed: bool,
}

#[derive(Debug, Clone)]
struct PlotText {
    text: String,
    /// Normalized position (0..1) relative to plot area
    nx: f64,
    ny: f64,
}

#[derive(Debug, Clone)]
struct AxisInfo {
    label: String,
    ticks: Vec<f64>,
}

#[derive(Debug)]
struct PlotData {
    title: String,
    points: Vec<ScatterPoint>,
    lines: Vec<PlotLine>,
    annotations: Vec<PlotText>,
    x_axis: AxisInfo,
    y_axis: AxisInfo,
    x_range: (f64, f64),
    y_range: (f64, f64),
    /// Legend entries: (label, color)
    legend: Vec<(String, Color)>,
}

// ── PDF parsing ─────────────────────────────────────────────────────

fn rgb_to_color(r: f64, g: f64, b: f64) -> Color {
    Color::Rgb(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn is_gray(r: f64, g: f64, b: f64) -> bool {
    (r - g).abs() < 0.05 && (g - b).abs() < 0.05
}

/// Color identity for grouping — bucket to 2 decimal places
fn color_key(r: f64, g: f64, b: f64) -> (i32, i32, i32) {
    ((r * 100.0) as i32, (g * 100.0) as i32, (b * 100.0) as i32)
}

pub fn extract_plot_data(pdf_path: &Path) -> Option<PlotData> {
    let pdf_bytes = std::fs::read(pdf_path).ok()?;
    let text_info = extract_text_bbox(pdf_path);

    // Decompress PDF streams
    let streams = decompress_streams(&pdf_bytes);
    if streams.is_empty() {
        return None;
    }

    let main_stream = &streams[0];

    // Parse grid lines to find plot area bounds and tick values
    let (plot_bounds, x_ticks_pdf, y_ticks_pdf) = find_plot_bounds(main_stream);
    let (px_min, px_max, py_min, py_max) = plot_bounds?;

    // Extract tick values from text near the axes
    let (x_tick_values, y_tick_values) = extract_tick_values(
        &text_info,
        px_min, px_max, py_min, py_max,
        &x_ticks_pdf, &y_ticks_pdf,
    );

    // Determine data ranges from tick values
    let x_range = if x_tick_values.len() >= 2 {
        let min = x_tick_values.first().copied().unwrap_or(0.0);
        let max = x_tick_values.last().copied().unwrap_or(1.0);
        (min, max)
    } else {
        (0.0, 1.0)
    };
    let y_range = if y_tick_values.len() >= 2 {
        let min = y_tick_values.first().copied().unwrap_or(0.0);
        let max = y_tick_values.last().copied().unwrap_or(1.0);
        (min, max)
    } else {
        (0.0, 1.0)
    };

    let pdf_to_data_x = |px: f64| -> f64 {
        x_range.0 + (px - px_min) / (px_max - px_min) * (x_range.1 - x_range.0)
    };
    let pdf_to_data_y = |py: f64| -> f64 {
        y_range.0 + (py - py_min) / (py_max - py_min) * (y_range.1 - y_range.0)
    };

    // Extract scatter markers (small colored filled paths)
    let markers = extract_markers(main_stream, px_min, px_max, py_min, py_max);
    let points: Vec<ScatterPoint> = markers
        .iter()
        .map(|m| ScatterPoint {
            x: pdf_to_data_x(m.0),
            y: pdf_to_data_y(m.1),
            color: rgb_to_color(m.2, m.3, m.4),
        })
        .collect();

    // Extract dashed lines
    let dashed_lines = extract_dashed_lines(main_stream, px_min, px_max, py_min, py_max);
    let lines: Vec<PlotLine> = dashed_lines
        .iter()
        .map(|dl| PlotLine {
            points: dl.points.iter().map(|&(px, py)| (pdf_to_data_x(px), pdf_to_data_y(py))).collect(),
            color: if dl.is_black { Color::Gray } else { Color::DarkGray },
            dashed: true,
        })
        .collect();

    // Extract title, axis labels, annotations from text
    let page_height = extract_page_height(&pdf_bytes).unwrap_or(315.0);
    let title = extract_title_text(&text_info, px_min, px_max, py_min, page_height);
    let x_label = extract_x_label(&text_info, px_min, px_max, py_max, page_height);
    let y_label = extract_y_label(&text_info, px_min, py_min, py_max, page_height);
    let annotations = extract_annotations(
        &text_info, px_min, px_max, py_min, py_max, page_height,
    );

    // Build legend from unique marker colors
    let legend = build_legend(&text_info, &markers, px_max, page_height);

    Some(PlotData {
        title,
        points,
        lines,
        annotations,
        x_axis: AxisInfo { label: x_label, ticks: x_tick_values },
        y_axis: AxisInfo { label: y_label, ticks: y_tick_values },
        x_range,
        y_range,
        legend,
    })
}

// ── PDF stream decompression ────────────────────────────────────────

fn decompress_streams(pdf_bytes: &[u8]) -> Vec<String> {
    use std::io::Read;
    let mut streams = Vec::new();

    // Find stream markers
    let mut pos = 0;
    while pos < pdf_bytes.len() {
        if let Some(start) = find_bytes(&pdf_bytes[pos..], b"stream\n")
            .or_else(|| find_bytes(&pdf_bytes[pos..], b"stream\r\n"))
        {
            let abs_start = pos + start + if pdf_bytes[pos + start + 6] == b'\r' { 8 } else { 7 };
            if let Some(end) = find_bytes(&pdf_bytes[abs_start..], b"endstream") {
                let raw = &pdf_bytes[abs_start..abs_start + end];
                // Try zlib decompression
                let mut decoder = flate2::read::ZlibDecoder::new(raw);
                let mut decoded = String::new();
                if decoder.read_to_string(&mut decoded).is_ok() {
                    streams.push(decoded);
                } else if let Ok(s) = String::from_utf8(raw.to_vec()) {
                    streams.push(s);
                }
                pos = abs_start + end;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    streams
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── Plot bounds detection ───────────────────────────────────────────

/// Returns (plot_bounds, x_tick_pdf_positions, y_tick_pdf_positions)
fn find_plot_bounds(stream: &str) -> (Option<(f64, f64, f64, f64)>, Vec<f64>, Vec<f64>) {
    // Look for the clip rectangle "X Y W H re W n" which defines the plot area
    let re_pattern = regex::Regex::new(
        r"([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+re\s+W\s+n"
    ).unwrap();

    let mut plot_rect = None;
    for cap in re_pattern.captures_iter(stream) {
        let x: f64 = cap[1].parse().unwrap_or(0.0);
        let y: f64 = cap[2].parse().unwrap_or(0.0);
        let w: f64 = cap[3].parse().unwrap_or(0.0);
        let h: f64 = cap[4].parse().unwrap_or(0.0);
        // The plot clip rect is typically the largest one
        if w > 50.0 && h > 50.0 {
            plot_rect = Some((x, x + w, y, y + h));
            break;
        }
    }

    let (px_min, px_max, py_min, py_max) = match plot_rect {
        Some(r) => r,
        None => return (None, Vec::new(), Vec::new()),
    };

    // Find x-axis tick positions: vertical grid lines within the plot area
    // Pattern: "X py_min m X py_max l S" (vertical line spanning full height)
    let mut x_ticks = Vec::new();
    let mut y_ticks = Vec::new();

    // Look for tick marks: short lines from axis edge
    // X ticks: "X py_min m X (py_min - 3.5) l"
    let tick_re = regex::Regex::new(
        r"([\d.]+)\s+([\d.]+)\s+m\s*\n\s*([\d.]+)\s+([\d.]+)\s+l"
    ).unwrap();

    for cap in tick_re.captures_iter(stream) {
        let x1: f64 = cap[1].parse().unwrap_or(0.0);
        let y1: f64 = cap[2].parse().unwrap_or(0.0);
        let x2: f64 = cap[3].parse().unwrap_or(0.0);
        let y2: f64 = cap[4].parse().unwrap_or(0.0);

        // X-axis tick: vertical short line at bottom of plot
        if (x1 - x2).abs() < 0.01 && (y1 - py_min).abs() < 1.0 && (y1 - y2).abs() < 5.0 && y2 < py_min {
            if x1 >= px_min - 1.0 && x1 <= px_max + 1.0 && !x_ticks.contains(&x1) {
                x_ticks.push(x1);
            }
        }

        // Y-axis tick: horizontal short line at left of plot
        if (y1 - y2).abs() < 0.01 && (x1 - px_min).abs() < 1.0 && (x1 - x2).abs() < 5.0 && x2 < px_min {
            if y1 >= py_min - 1.0 && y1 <= py_max + 1.0 && !y_ticks.contains(&y1) {
                y_ticks.push(y1);
            }
        }
    }

    x_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    y_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());

    (plot_rect, x_ticks, y_ticks)
}

// ── Text extraction via pdftotext ───────────────────────────────────

#[derive(Debug, Clone)]
struct TextWord {
    text: String,
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
}

fn extract_text_bbox(pdf_path: &Path) -> Vec<TextWord> {
    let output = Command::new("pdftotext")
        .args(["-bbox", &pdf_path.display().to_string(), "-"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let html = String::from_utf8_lossy(&output.stdout);
    let mut words = Vec::new();

    let word_re = regex::Regex::new(
        r#"<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">(.*?)</word>"#
    ).unwrap();

    for cap in word_re.captures_iter(&html) {
        words.push(TextWord {
            x_min: cap[1].parse().unwrap_or(0.0),
            y_min: cap[2].parse().unwrap_or(0.0),
            x_max: cap[3].parse().unwrap_or(0.0),
            y_max: cap[4].parse().unwrap_or(0.0),
            text: cap[5].to_string(),
        });
    }

    words
}

fn extract_page_height(pdf_bytes: &[u8]) -> Option<f64> {
    let text = String::from_utf8_lossy(pdf_bytes);
    // Look for /MediaBox [0 0 W H]
    let re = regex::Regex::new(r"/MediaBox\s*\[\s*[\d.]+\s+[\d.]+\s+([\d.]+)\s+([\d.]+)\s*\]").ok()?;
    re.captures(&text).and_then(|c| c[2].parse().ok())
}

fn extract_tick_values(
    words: &[TextWord],
    px_min: f64, px_max: f64, py_min: f64, py_max: f64,
    x_ticks_pdf: &[f64], y_ticks_pdf: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    // X tick values: text below the plot area, aligned with tick positions
    // pdftotext y-coords are from top of page; PDF coords are from bottom.
    // We need the page height to convert. For now, match by x-position proximity.
    let mut x_vals: Vec<(f64, f64)> = Vec::new(); // (pdf_x_pos, value)
    let mut y_vals: Vec<(f64, f64)> = Vec::new(); // (pdf_y_pos, value)

    for word in words {
        if let Ok(val) = word.text.trim().parse::<f64>() {
            let word_cx = (word.x_min + word.x_max) / 2.0;

            // Check if this number is near an x-axis tick position
            for &tick_x in x_ticks_pdf {
                if (word_cx - tick_x).abs() < 8.0 {
                    x_vals.push((tick_x, val));
                    break;
                }
            }

            // Check if near a y-axis tick (word is to the left of plot)
            if word.x_max < px_min + 5.0 && word.x_min < px_min {
                let word_cy = (word.y_min + word.y_max) / 2.0;
                for &tick_y in y_ticks_pdf {
                    // pdftotext y is from page top; need to check proximity
                    // We'll pair by sorted order later
                    y_vals.push((word_cy, val));
                    break;
                }
            }
        }
    }

    // Sort and deduplicate
    x_vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    x_vals.dedup_by(|a, b| (a.0 - b.0).abs() < 3.0);

    y_vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    y_vals.dedup_by(|a, b| (a.0 - b.0).abs() < 3.0);
    // pdftotext y increases downward, but data y increases upward, so reverse
    y_vals.reverse();

    // Pair y-axis text values with PDF tick positions (both sorted bottom-to-top)
    let mut final_y_ticks: Vec<f64> = y_vals.iter().map(|v| v.1).collect();
    final_y_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut final_x_ticks: Vec<f64> = x_vals.iter().map(|v| v.1).collect();
    final_x_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());

    (final_x_ticks, final_y_ticks)
}

// ── Marker extraction ───────────────────────────────────────────────

struct MarkerHit(f64, f64, f64, f64, f64); // x, y, r, g, b

fn extract_markers(
    stream: &str,
    px_min: f64, px_max: f64, py_min: f64, py_max: f64,
) -> Vec<MarkerHit> {
    let mut markers = Vec::new();
    let tokens: Vec<&str> = stream.split_whitespace().collect();

    // Track graphics state
    let mut state_stack: Vec<(f64, f64, f64, f64, f64)> = Vec::new(); // (tx, ty, r, g, b)
    let mut tx = 0.0f64;
    let mut ty = 0.0f64;
    let mut fill_r = 0.0f64;
    let mut fill_g = 0.0f64;
    let mut fill_b = 0.0f64;

    // Track current path for small-shape detection
    let mut path_points: Vec<(f64, f64)> = Vec::new();
    let mut in_path = false;

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        match t {
            "q" => {
                state_stack.push((tx, ty, fill_r, fill_g, fill_b));
            }
            "Q" => {
                if let Some((stx, sty, sr, sg, sb)) = state_stack.pop() {
                    tx = stx;
                    ty = sty;
                    fill_r = sr;
                    fill_g = sg;
                    fill_b = sb;
                }
            }
            "cm" if i >= 6 => {
                if let (Ok(a), Ok(d), Ok(etx), Ok(ety)) = (
                    tokens[i - 6].parse::<f64>(),
                    tokens[i - 3].parse::<f64>(),
                    tokens[i - 2].parse::<f64>(),
                    tokens[i - 1].parse::<f64>(),
                ) {
                    if (a - 1.0).abs() < 0.01 && (d - 1.0).abs() < 0.01 {
                        tx += etx;
                        ty += ety;
                    }
                }
            }
            "rg" | "sc" if i >= 3 => {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    tokens[i - 3].parse::<f64>(),
                    tokens[i - 2].parse::<f64>(),
                    tokens[i - 1].parse::<f64>(),
                ) {
                    fill_r = r;
                    fill_g = g;
                    fill_b = b;
                }
            }
            "g" if i >= 1 => {
                if let Ok(v) = tokens[i - 1].parse::<f64>() {
                    fill_r = v;
                    fill_g = v;
                    fill_b = v;
                }
            }
            "m" if i >= 2 => {
                if let (Ok(x), Ok(y)) = (
                    tokens[i - 2].parse::<f64>(),
                    tokens[i - 1].parse::<f64>(),
                ) {
                    path_points = vec![(x, y)];
                    in_path = true;
                }
            }
            "l" if i >= 2 && in_path => {
                if let (Ok(x), Ok(y)) = (
                    tokens[i - 2].parse::<f64>(),
                    tokens[i - 1].parse::<f64>(),
                ) {
                    path_points.push((x, y));
                }
            }
            "c" if i >= 6 && in_path => {
                // Bezier: take endpoint
                if let (Ok(x), Ok(y)) = (
                    tokens[i - 2].parse::<f64>(),
                    tokens[i - 1].parse::<f64>(),
                ) {
                    path_points.push((x, y));
                }
            }
            "h" if in_path => {} // close path
            "B" | "f" | "b" | "B*" | "f*" if in_path => {
                in_path = false;
                if path_points.len() >= 3 {
                    let xs: Vec<f64> = path_points.iter().map(|p| p.0).collect();
                    let ys: Vec<f64> = path_points.iter().map(|p| p.1).collect();
                    let w = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                        - xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let h = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                        - ys.iter().cloned().fold(f64::INFINITY, f64::min);

                    // Small shape = marker
                    if w < 25.0 && h < 25.0 && w > 0.5 && !is_gray(fill_r, fill_g, fill_b) {
                        let cx = tx + (xs.iter().cloned().fold(f64::INFINITY, f64::min)
                            + xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) / 2.0;
                        let cy = ty + (ys.iter().cloned().fold(f64::INFINITY, f64::min)
                            + ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) / 2.0;

                        // Only include markers within plot area (with margin for edge markers)
                        if cx >= px_min - 10.0 && cx <= px_max + 10.0
                            && cy >= py_min - 10.0 && cy <= py_max + 10.0
                        {
                            markers.push(MarkerHit(cx, cy, fill_r, fill_g, fill_b));
                        }
                    }
                }
                path_points.clear();
            }
            "S" if in_path => {
                in_path = false;
                path_points.clear();
            }
            "Do" if i >= 1 => {
                // XObject form placement — treat as marker at current position
                let name = tokens[i - 1];
                if name.starts_with("/P") || name.starts_with("/M") {
                    if tx >= px_min - 10.0 && tx <= px_max + 10.0
                        && ty >= py_min - 10.0 && ty <= py_max + 10.0
                        && !is_gray(fill_r, fill_g, fill_b)
                    {
                        markers.push(MarkerHit(tx, ty, fill_r, fill_g, fill_b));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    markers
}

// ── Dashed line extraction ──────────────────────────────────────────

struct DashedLine {
    points: Vec<(f64, f64)>,
    is_black: bool,
}

fn extract_dashed_lines(
    stream: &str,
    px_min: f64, px_max: f64, py_min: f64, py_max: f64,
) -> Vec<DashedLine> {
    let mut lines = Vec::new();

    // Parse the stream sequentially, tracking dash state and building polylines.
    // A "real" reference line (Pareto frontier, baseline) is a connected polyline
    // that spans a significant portion of the plot width — not a tiny rectangle,
    // grid line, tick mark, or legend decoration.
    let tokens: Vec<&str> = stream.split_whitespace().collect();

    let mut dash_active = false;
    let mut stroke_gray: f64 = 0.0; // 0 = black, 1 = white
    let mut current_path: Vec<(f64, f64)> = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        match t {
            "d" if i >= 2 => {
                // "[...] phase d" — check if it's a real dash or a reset "[] 0 d"
                // Walk backwards to find the "["
                let mut found_dash = false;
                for j in (0..i).rev() {
                    if tokens[j].starts_with('[') {
                        let bracket_content: String = tokens[j..i-1].join(" ");
                        let inner = bracket_content.trim_start_matches('[').trim_end_matches(']');
                        // Non-empty bracket = real dash pattern
                        found_dash = !inner.trim().is_empty()
                            && inner.contains(|c: char| c.is_ascii_digit());
                        break;
                    }
                }
                dash_active = found_dash;
            }
            "G" if i >= 1 => {
                if let Ok(v) = tokens[i - 1].parse::<f64>() {
                    stroke_gray = v;
                }
            }
            "m" if i >= 2 => {
                if let (Ok(x), Ok(y)) = (tokens[i-2].parse::<f64>(), tokens[i-1].parse::<f64>()) {
                    // New subpath — if we had a good polyline, emit it
                    maybe_emit_line(&current_path, dash_active, stroke_gray,
                                    px_min, px_max, py_min, py_max, &mut lines);
                    current_path = vec![(x, y)];
                }
            }
            "l" if i >= 2 => {
                if let (Ok(x), Ok(y)) = (tokens[i-2].parse::<f64>(), tokens[i-1].parse::<f64>()) {
                    current_path.push((x, y));
                }
            }
            "S" | "s" => {
                maybe_emit_line(&current_path, dash_active, stroke_gray,
                                px_min, px_max, py_min, py_max, &mut lines);
                current_path.clear();
            }
            "h" => {
                // Close path — reference lines are open, so closed paths are rectangles/markers
                current_path.clear();
            }
            _ => {}
        }
        i += 1;
    }

    lines
}

fn maybe_emit_line(
    points: &[(f64, f64)],
    dash_active: bool,
    stroke_gray: f64,
    px_min: f64, px_max: f64, py_min: f64, py_max: f64,
    out: &mut Vec<DashedLine>,
) {
    if !dash_active || points.len() < 2 {
        return;
    }

    // Filter to points inside the plot area
    let plot_points: Vec<(f64, f64)> = points
        .iter()
        .filter(|&&(x, y)| {
            x >= px_min - 2.0 && x <= px_max + 2.0
                && y >= py_min - 2.0 && y <= py_max + 2.0
        })
        .copied()
        .collect();

    if plot_points.len() < 2 {
        return;
    }

    // Compute bounding box of the polyline
    let xs: Vec<f64> = plot_points.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = plot_points.iter().map(|p| p.1).collect();
    let width = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let height = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - ys.iter().cloned().fold(f64::INFINITY, f64::min);

    let plot_width = px_max - px_min;
    let plot_height = py_max - py_min;

    // A real reference line spans a significant portion of the plot.
    // Reject:
    //  - Pure vertical or horizontal grid lines that span the full axis
    //  - Lines on the plot boundary (axis lines)
    //  - Tiny shapes (legend markers, tick marks)
    let is_horizontal_enough = width > plot_width * 0.15;
    let is_not_tiny = width > 15.0 || height > 15.0;

    // Reject lines that sit exactly on the plot boundary (axis lines)
    let y_min_pt = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max_pt = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let on_bottom_edge = (y_min_pt - py_min).abs() < 1.0 && (y_max_pt - py_min).abs() < 1.0;
    let on_top_edge = (y_min_pt - py_max).abs() < 1.0 && (y_max_pt - py_max).abs() < 1.0;
    let on_y_edge = on_bottom_edge || on_top_edge;

    let x_min_pt = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max_pt = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let on_left_edge = (x_min_pt - px_min).abs() < 1.0 && (x_max_pt - px_min).abs() < 1.0;
    let on_right_edge = (x_min_pt - px_max).abs() < 1.0 && (x_max_pt - px_max).abs() < 1.0;
    let on_x_edge = on_left_edge || on_right_edge;

    // Reject pure vertical grid lines (span full height)
    let is_full_span_vertical = width < 0.5 && height > plot_height * 0.95;
    // Note: we allow full-width horizontal lines — they could be baselines.
    // Axis-edge lines are already rejected above.

    if is_horizontal_enough && is_not_tiny
        && !on_y_edge && !on_x_edge
        && !is_full_span_vertical
    {
        out.push(DashedLine {
            points: plot_points,
            is_black: stroke_gray < 0.3,
        });
    }
}

// ── Text extraction helpers ─────────────────────────────────────────

fn extract_title_text(words: &[TextWord], px_min: f64, px_max: f64, _py_min: f64, _page_height: f64) -> String {
    // Title is above the plot area, centered horizontally
    // In pdftotext bbox coords, y increases downward from page top
    // Title words have small y values (near top of page)
    let title_words: Vec<&TextWord> = words
        .iter()
        .filter(|w| {
            let cx = (w.x_min + w.x_max) / 2.0;
            cx > px_min && cx < px_max && w.y_min < 25.0
        })
        .collect();

    title_words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn extract_x_label(words: &[TextWord], px_min: f64, px_max: f64, _py_max: f64, _page_height: f64) -> String {
    // X-axis label: centered below axis, large y value in pdftotext coords
    let label_words: Vec<&TextWord> = words
        .iter()
        .filter(|w| {
            let cx = (w.x_min + w.x_max) / 2.0;
            cx > px_min && cx < px_max && w.y_min > 290.0
        })
        .collect();

    label_words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn extract_y_label(words: &[TextWord], px_min: f64, _py_min: f64, _py_max: f64, _page_height: f64) -> String {
    // Y-axis label: rotated text to the left of the plot
    // These words have very small x values
    let label_words: Vec<&TextWord> = words
        .iter()
        .filter(|w| w.x_max < px_min - 15.0 && w.x_min < 20.0)
        .collect();

    label_words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn extract_annotations(
    words: &[TextWord],
    px_min: f64, px_max: f64, py_min: f64, py_max: f64,
    _page_height: f64,
) -> Vec<PlotText> {
    // Annotations are text inside the plot area that isn't a tick label
    // For matplotlib, these are things like "Landau baseline"
    let mut annotations = Vec::new();

    for word in words {
        let cx = (word.x_min + word.x_max) / 2.0;
        let cy = (word.y_min + word.y_max) / 2.0;

        // Inside plot area (using pdftotext coords which match PDF for x, but y is flipped)
        // We can't perfectly match without page height, but annotations are between axis labels
        if cx > px_min + 10.0 && cx < px_max - 10.0
            && cy > 30.0 && cy < 280.0
        {
            // Skip if it's a number (tick label)
            if word.text.trim().parse::<f64>().is_ok() {
                continue;
            }
            // Normalize position
            let nx = (cx - px_min) / (px_max - px_min);
            // Rough y normalization (pdftotext y increases downward)
            let ny = 1.0 - (cy - 30.0) / 250.0;
            annotations.push(PlotText {
                text: word.text.clone(),
                nx: nx.clamp(0.0, 1.0),
                ny: ny.clamp(0.0, 1.0),
            });
        }
    }

    annotations
}

fn build_legend(
    words: &[TextWord],
    markers: &[MarkerHit],
    px_max: f64,
    _page_height: f64,
) -> Vec<(String, Color)> {
    // Legend text is to the right of the plot area
    let legend_words: Vec<&TextWord> = words
        .iter()
        .filter(|w| w.x_min > px_max)
        .collect();

    // Group markers by color, preserving insertion order
    let mut color_groups: Vec<(i32, i32, i32, Color)> = Vec::new();
    for m in markers {
        let key = color_key(m.2, m.3, m.4);
        if !color_groups.iter().any(|g| g.0 == key.0 && g.1 == key.1 && g.2 == key.2) {
            color_groups.push((key.0, key.1, key.2, rgb_to_color(m.2, m.3, m.4)));
        }
    }

    // Group nearby words into lines by y-position
    let mut legend_lines: Vec<(f64, Vec<String>)> = Vec::new();
    for word in &legend_words {
        let cy = (word.y_min + word.y_max) / 2.0;
        if let Some(line) = legend_lines.iter_mut().find(|l| (l.0 - cy).abs() < 5.0) {
            line.1.push(word.text.clone());
        } else {
            legend_lines.push((cy, vec![word.text.clone()]));
        }
    }
    legend_lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Collect non-header legend labels in order
    let mut labels: Vec<String> = Vec::new();
    for (_, words_on_line) in &legend_lines {
        let label = words_on_line.join(" ");
        // Skip section headers, decorative entries, and very short text
        if label.len() <= 2
            || label.starts_with("Archit")
            || label.starts_with("Sweep")
            || label.starts_with("Pareto")
            || label.contains("frontier")
            || label.contains("baseline")
        {
            continue;
        }
        labels.push(label);
    }

    // Match labels to color groups.
    // In matplotlib, the first N legend entries correspond to the N color groups
    // (architecture colors). Remaining entries (sweep marker shapes) reuse colors.
    let mut legend: Vec<(String, Color)> = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        if i < color_groups.len() {
            legend.push((label.clone(), color_groups[i].3));
        }
        // Stop after we've matched all color groups — the remaining
        // legend entries (sweep names) use marker shape, not color
    }

    // If we couldn't match any, use color groups with generic names
    if legend.is_empty() {
        for (i, (_, _, _, color)) in color_groups.iter().enumerate() {
            legend.push((format!("Series {}", i + 1), *color));
        }
    }

    legend
}

// ── Rendering ───────────────────────────────────────────────────────

pub fn render_plot(
    spec: &PlotSpec,
    base_dir: &Path,
    term_cols: u16,
    term_rows: u16,
) -> Vec<Line<'static>> {
    let pdf_path = base_dir.join(&spec.file);
    let data = match extract_plot_data(&pdf_path) {
        Some(d) => d,
        None => return vec![Line::raw(format!("[plot: cannot parse {}]", spec.file))],
    };

    if term_cols < 40 || term_rows < 10 {
        let title = spec.title.as_deref().unwrap_or(&data.title);
        return vec![Line::raw(format!("[Plot: {}]", title))];
    }

    let title = spec.title.as_deref().unwrap_or(&data.title);
    let show_legend = term_cols >= 80 && !data.legend.is_empty();

    // Layout
    let y_label_width: u16 = 8;
    let legend_width: u16 = if show_legend { 22 } else { 0 };
    let plot_cols = term_cols.saturating_sub(y_label_width + legend_width + 1);
    let plot_rows = term_rows.saturating_sub(5); // title + spacer + x-axis labels + x-axis title + legend

    if plot_cols < 10 || plot_rows < 4 {
        return vec![Line::raw(format!("[Plot: {}]", title))];
    }

    // Build a character grid for the plot area.
    // Each cell holds an optional (character, color).
    // Markers are placed as actual text characters; dashed lines use '╌' or '┄'.
    let (x_min, x_max) = data.x_range;
    let (y_min, y_max) = data.y_range;
    let pc = plot_cols as usize;
    let pr = plot_rows as usize;

    let mut grid: Vec<Vec<Option<(char, Color)>>> = vec![vec![None; pc]; pr];

    let data_to_col = |x: f64| -> isize {
        ((x - x_min) / (x_max - x_min) * (pc as f64 - 1.0)).round() as isize
    };
    let data_to_row = |y: f64| -> isize {
        ((1.0 - (y - y_min) / (y_max - y_min)) * (pr as f64 - 1.0)).round() as isize
    };

    // Draw dashed reference lines first (behind markers)
    for line in &data.lines {
        let ch = if line.dashed { '╌' } else { '─' };
        let color = line.color;
        for i in 0..line.points.len().saturating_sub(1) {
            let (x1, y1) = line.points[i];
            let (x2, y2) = line.points[i + 1];
            let c1 = data_to_col(x1);
            let r1 = data_to_row(y1);
            let c2 = data_to_col(x2);
            let r2 = data_to_row(y2);

            let steps = (c2 - c1).abs().max((r2 - r1).abs()).max(1) as usize;
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let c = (c1 as f64 + t * (c2 - c1) as f64).round() as isize;
                let r = (r1 as f64 + t * (r2 - r1) as f64).round() as isize;
                if c >= 0 && c < pc as isize && r >= 0 && r < pr as isize {
                    grid[r as usize][c as usize] = Some((ch, color));
                }
            }
        }
    }

    // Draw scatter points as characters — markers overwrite lines
    for pt in &data.points {
        let c = data_to_col(pt.x);
        let r = data_to_row(pt.y);
        if c >= 0 && c < pc as isize && r >= 0 && r < pr as isize {
            grid[r as usize][c as usize] = Some(('●', pt.color));
        }
    }

    // Render grid to Lines
    let mut plot_lines: Vec<Line<'static>> = Vec::new();
    for row in &grid {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run_text = String::new();
        let mut run_color: Option<Color> = None;

        for cell in row {
            match cell {
                Some((ch, color)) => {
                    if run_color == Some(*color) {
                        run_text.push(*ch);
                    } else {
                        if !run_text.is_empty() {
                            let style = match run_color {
                                Some(c) => Style::default().fg(c),
                                None => Style::default(),
                            };
                            spans.push(Span::styled(run_text.clone(), style));
                            run_text.clear();
                        }
                        run_color = Some(*color);
                        run_text.push(*ch);
                    }
                }
                None => {
                    if run_color.is_some() {
                        let style = match run_color {
                            Some(c) => Style::default().fg(c),
                            None => Style::default(),
                        };
                        spans.push(Span::styled(run_text.clone(), style));
                        run_text.clear();
                        run_color = None;
                    }
                    run_text.push(' ');
                }
            }
        }
        if !run_text.is_empty() {
            let style = match run_color {
                Some(c) => Style::default().fg(c),
                None => Style::default(),
            };
            spans.push(Span::styled(run_text, style));
        }

        plot_lines.push(Line::from(spans));
    }

    // Compose output
    let mut output: Vec<Line<'static>> = Vec::new();

    // Title
    let title_spans = vec![Span::styled(
        title.to_string(),
        Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD),
    )];
    // We'll handle centering in the caller via RenderText alignment
    // For RenderImage, we compose the full line ourselves
    let title_pad = (term_cols as usize).saturating_sub(title.len()) / 2;
    output.push(Line::from(vec![
        Span::raw(" ".repeat(title_pad)),
        Span::styled(title.to_string(), Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)),
    ]));

    // Spacer
    output.push(Line::raw(""));

    // Select y-axis ticks to display based on available height
    let y_ticks = select_ticks(&data.y_axis.ticks, plot_rows as usize);

    // Plot rows: y-label | border | braille | legend
    for (row_idx, braille_line) in plot_lines.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();

        // Y-axis label
        let y_frac = row_idx as f64 / (plot_rows as f64 - 1.0).max(1.0);
        let y_val = y_max - y_frac * (y_max - y_min);

        let mut y_label = String::new();
        let mut has_tick = false;
        for &tick in &y_ticks {
            let tick_frac = 1.0 - (tick - y_min) / (y_max - y_min);
            let tick_row = (tick_frac * (plot_rows as f64 - 1.0)).round() as usize;
            if tick_row == row_idx {
                y_label = format_tick_value(tick);
                has_tick = true;
                break;
            }
        }

        let label_str = if has_tick {
            format!("{:>6} ", y_label)
        } else {
            "       ".to_string()
        };
        spans.push(Span::styled(label_str, Style::default().fg(Color::DarkGray)));

        // Border character
        let border = if row_idx == plot_rows as usize - 1 {
            "┗"
        } else if has_tick {
            "┤"
        } else {
            "│"
        };
        spans.push(Span::styled(border, Style::default().fg(Color::DarkGray)));

        // Braille content
        spans.extend(braille_line.spans.iter().cloned());

        // Legend (right side)
        if show_legend && row_idx < data.legend.len() + 4 {
            let legend_text = match row_idx {
                0 => Some(("", Color::Reset)),
                r if r >= 1 && r <= data.legend.len() => {
                    // Will be handled below with marker
                    None
                }
                _ => None,
            };

            if row_idx >= 1 && row_idx <= data.legend.len() {
                let (ref label, color) = data.legend[row_idx - 1];
                spans.push(Span::raw(" "));
                spans.push(Span::styled("● ", Style::default().fg(color)));
                let truncated = if label.len() > 18 { &label[..18] } else { label };
                spans.push(Span::styled(truncated.to_string(), Style::default().fg(Color::Gray)));
            } else if let Some((text, _)) = legend_text {
                spans.push(Span::raw(format!(" {}", text)));
            }
        }

        output.push(Line::from(spans));
    }

    // X-axis bottom border
    let mut x_axis_spans: Vec<Span<'static>> = Vec::new();
    x_axis_spans.push(Span::raw("       "));
    x_axis_spans.push(Span::styled(" ", Style::default().fg(Color::DarkGray)));

    let x_ticks = select_ticks(&data.x_axis.ticks, plot_cols as usize / 8);
    let mut axis_chars: Vec<char> = vec!['━'; plot_cols as usize];
    for &tick in &x_ticks {
        let tick_frac = (tick - x_min) / (x_max - x_min);
        let col = (tick_frac * (plot_cols as f64 - 1.0)).round() as usize;
        if col < axis_chars.len() {
            axis_chars[col] = '┴';
        }
    }
    x_axis_spans.push(Span::styled(
        axis_chars.iter().collect::<String>(),
        Style::default().fg(Color::DarkGray),
    ));
    output.push(Line::from(x_axis_spans));

    // X-axis tick labels
    let mut label_buf = vec![' '; term_cols as usize];
    for &tick in &x_ticks {
        let tick_frac = (tick - x_min) / (x_max - x_min);
        let col = (tick_frac * (plot_cols as f64 - 1.0)).round() as usize + y_label_width as usize + 1;
        let label = format_tick_value(tick);
        let start = col.saturating_sub(label.len() / 2);
        for (j, ch) in label.chars().enumerate() {
            let pos = start + j;
            if pos < label_buf.len() {
                label_buf[pos] = ch;
            }
        }
    }
    output.push(Line::from(vec![Span::styled(
        label_buf.iter().collect::<String>(),
        Style::default().fg(Color::DarkGray),
    )]));

    // X-axis title
    if !data.x_axis.label.is_empty() {
        let pad = (term_cols as usize).saturating_sub(data.x_axis.label.len()) / 2;
        output.push(Line::from(vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(data.x_axis.label.clone(), Style::default().fg(Color::Gray)),
        ]));
    }

    output
}

fn select_ticks(all_ticks: &[f64], max_count: usize) -> Vec<f64> {
    if all_ticks.len() <= max_count {
        return all_ticks.to_vec();
    }
    // Take evenly spaced subset, always including first and last
    let mut selected = Vec::new();
    let step = (all_ticks.len() - 1) as f64 / (max_count - 1).max(1) as f64;
    for i in 0..max_count {
        let idx = (i as f64 * step).round() as usize;
        let idx = idx.min(all_ticks.len() - 1);
        if !selected.contains(&all_ticks[idx]) {
            selected.push(all_ticks[idx]);
        }
    }
    selected
}

fn format_tick_value(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{:.0}", v)
    } else if v.abs() >= 1.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.3}", v)
    }
}
