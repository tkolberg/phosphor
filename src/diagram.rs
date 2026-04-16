use std::collections::HashMap;

use crate::elements::StyledText;

#[derive(Debug)]
struct Node {
    label: String,
}

#[derive(Debug)]
struct Edge {
    from: usize,
    to: usize,
}

#[derive(Debug)]
struct Diagram {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

/// Parse the DSL into a diagram structure.
/// Format: `[Box A] -> [Box B] -> [Box C]` per line.
fn parse_diagram(input: &str) -> Diagram {
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<Edge> = Vec::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split("->").collect();
        let mut chain: Vec<usize> = Vec::new();

        for part in &parts {
            let part = part.trim();
            if let Some(label) = extract_bracket_label(part) {
                let id = if let Some(&existing) = node_map.get(&label) {
                    existing
                } else {
                    let id = nodes.len();
                    nodes.push(Node {
                        label: label.clone(),
                    });
                    node_map.insert(label, id);
                    id
                };
                chain.push(id);
            }
        }

        for window in chain.windows(2) {
            let edge = Edge {
                from: window[0],
                to: window[1],
            };
            if !edges.iter().any(|e| e.from == edge.from && e.to == edge.to) {
                edges.push(edge);
            }
        }
    }

    Diagram { nodes, edges }
}

fn extract_bracket_label(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') {
        Some(s[1..s.len() - 1].trim().to_string())
    } else {
        None
    }
}

// --- Width-aware layout ---

const ARROW_GAP: usize = 5; // space between boxes for "────→"

fn node_box_width(label: &str) -> usize {
    label.chars().count() + 4
}

#[derive(Debug, Clone)]
struct NodeLayout {
    x: usize,
    row: usize,
    box_width: usize,
}

#[derive(Debug)]
struct Layout {
    nodes: Vec<NodeLayout>,
    num_rows: usize,
}

/// Width-aware layout: sizes boxes per-node and wraps chains when they exceed available width.
fn layout_diagram(diagram: &Diagram, available_width: u16) -> Layout {
    let width = available_width as usize;

    if diagram.nodes.is_empty() {
        return Layout {
            nodes: vec![],
            num_rows: 0,
        };
    }

    let box_widths: Vec<usize> = diagram
        .nodes
        .iter()
        .map(|n| node_box_width(&n.label))
        .collect();

    let mut node_layouts: Vec<Option<NodeLayout>> = vec![None; diagram.nodes.len()];
    let mut current_row: usize = 0;

    // Build adjacency
    let mut successors: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut has_predecessor = vec![false; diagram.nodes.len()];

    for edge in &diagram.edges {
        successors.entry(edge.from).or_default().push(edge.to);
        has_predecessor[edge.to] = true;
    }

    let roots: Vec<usize> = (0..diagram.nodes.len())
        .filter(|&i| !has_predecessor[i])
        .collect();
    let roots = if roots.is_empty() { vec![0] } else { roots };

    // Phase 1: Place the longest chain first (the "spine")
    // Find the root that produces the longest chain
    let mut best_chain: Vec<usize> = Vec::new();
    for &root in &roots {
        let mut chain = Vec::new();
        let mut current = root;
        let mut visited = vec![false; diagram.nodes.len()];
        loop {
            if visited[current] {
                break;
            }
            visited[current] = true;
            chain.push(current);
            if let Some(succs) = successors.get(&current) {
                if let Some(&next) = succs.first() {
                    if !visited[next] {
                        current = next;
                        continue;
                    }
                }
            }
            break;
        }
        if chain.len() > best_chain.len() {
            best_chain = chain;
        }
    }

    // Place the spine, wrapping as needed
    let mut x: usize = 0;
    for &node_idx in &best_chain {
        let bw = box_widths[node_idx];
        if x > 0 {
            if x + ARROW_GAP + bw > width {
                current_row += 1;
                x = 0;
            } else {
                x += ARROW_GAP;
            }
        }
        node_layouts[node_idx] = Some(NodeLayout {
            x,
            row: current_row,
            box_width: bw,
        });
        x += bw;
    }
    current_row += 1;

    // Phase 2: Place remaining roots (feeder nodes) — stack them on their own rows
    for &root in &roots {
        if node_layouts[root].is_some() {
            continue;
        }
        node_layouts[root] = Some(NodeLayout {
            x: 0,
            row: current_row,
            box_width: box_widths[root],
        });
        current_row += 1;
    }

    // Phase 3: Place any remaining unplaced nodes
    // Try to place them on the same row as a sibling (another node with the same source)
    // or on a new row next to their source
    for i in 0..diagram.nodes.len() {
        if node_layouts[i].is_some() {
            continue;
        }
        // Find a placed predecessor and try to place near a sibling
        let mut placed = false;
        for edge in &diagram.edges {
            if edge.to == i {
                if let Some(src_layout) = node_layouts[edge.from].clone() {
                    // Find which row has siblings (other successors of the same source)
                    let src_row = src_layout.row;
                    // Try to place on the same row as another output of the same source
                    if let Some(succs) = successors.get(&edge.from) {
                        for &sib in succs {
                            if sib != i {
                                if let Some(sib_layout) = node_layouts[sib].clone() {
                                    // Place next to sibling on the same row
                                    let sib_end = sib_layout.x + sib_layout.box_width;
                                    let bw = box_widths[i];
                                    if sib_end + ARROW_GAP + bw <= width {
                                        node_layouts[i] = Some(NodeLayout {
                                            x: sib_end + ARROW_GAP,
                                            row: sib_layout.row,
                                            box_width: bw,
                                        });
                                        placed = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if !placed {
                        // Place below the source
                        let bw = box_widths[i];
                        let target_x = src_layout.x;
                        node_layouts[i] = Some(NodeLayout {
                            x: target_x,
                            row: src_row + 1,
                            box_width: bw,
                        });
                        placed = true;
                    }
                    break;
                }
            }
        }
        if !placed {
            node_layouts[i] = Some(NodeLayout {
                x: 0,
                row: current_row,
                box_width: box_widths[i],
            });
            current_row += 1;
        }
    }

    // Recalculate num_rows
    let num_rows = node_layouts
        .iter()
        .map(|n| n.as_ref().unwrap().row)
        .max()
        .unwrap_or(0)
        + 1;

    Layout {
        nodes: node_layouts.into_iter().map(|n| n.unwrap()).collect(),
        num_rows,
    }
}

/// Render a diagram to styled text lines using box-drawing characters.
/// Uses a 2D character grid so all edges (same-row, cross-row, converging) render correctly.
pub fn render_diagram(input: &str, available_width: u16) -> Vec<StyledText> {
    let diagram = parse_diagram(input);
    if diagram.nodes.is_empty() {
        return vec![StyledText::plain("[empty diagram]")];
    }

    let layout = layout_diagram(&diagram, available_width);
    let width = available_width as usize;

    // Each row occupies 3 lines (top, mid, bot) + 2 connector lines between rows
    let row_height = 3usize;
    let gap_height = 2usize;
    let total_height = layout.num_rows * row_height + layout.num_rows.saturating_sub(1) * gap_height;

    // Build a 2D character grid
    let mut grid: Vec<Vec<char>> = vec![vec![' '; width]; total_height];

    // Helper: get the y-offset for the start of a row's box lines
    let row_y = |row: usize| -> usize {
        row * (row_height + gap_height)
    };

    // Group nodes by row
    let mut row_nodes: Vec<Vec<usize>> = vec![Vec::new(); layout.num_rows];
    for (idx, nl) in layout.nodes.iter().enumerate() {
        row_nodes[nl.row].push(idx);
    }
    for rn in &mut row_nodes {
        rn.sort_by_key(|&idx| layout.nodes[idx].x);
    }

    // Phase 1: Draw all boxes
    for (idx, nl) in layout.nodes.iter().enumerate() {
        let y = row_y(nl.row);
        let label = &diagram.nodes[idx].label;
        let inner_w = nl.box_width - 2;

        if nl.x + nl.box_width > width || y + 2 >= total_height {
            continue; // skip if out of bounds
        }

        // Top: ┌───┐
        grid[y][nl.x] = '┌';
        for j in 1..=inner_w {
            grid[y][nl.x + j] = '─';
        }
        grid[y][nl.x + nl.box_width - 1] = '┐';

        // Mid: │ Label │
        grid[y + 1][nl.x] = '│';
        let padded = format!("{:^width$}", label, width = inner_w);
        for (j, ch) in padded.chars().enumerate() {
            if nl.x + 1 + j < width {
                grid[y + 1][nl.x + 1 + j] = ch;
            }
        }
        grid[y + 1][nl.x + nl.box_width - 1] = '│';

        // Bot: └───┘
        grid[y + 2][nl.x] = '└';
        for j in 1..=inner_w {
            grid[y + 2][nl.x + j] = '─';
        }
        grid[y + 2][nl.x + nl.box_width - 1] = '┘';
    }

    // Phase 2: Draw all edges
    let edge_set: Vec<(usize, usize)> = diagram.edges.iter().map(|e| (e.from, e.to)).collect();

    for &(from, to) in &edge_set {
        let from_nl = &layout.nodes[from];
        let to_nl = &layout.nodes[to];

        if from_nl.row == to_nl.row {
            // Same-row horizontal arrow
            let from_right = from_nl.x + from_nl.box_width;
            let to_left = to_nl.x;
            let y = row_y(from_nl.row) + 1; // mid line

            if to_left > from_right && from_nl.x < to_nl.x {
                for j in from_right..to_left.saturating_sub(1) {
                    if j < width {
                        grid[y][j] = '─';
                    }
                }
                if to_left > 0 && to_left - 1 < width {
                    grid[y][to_left - 1] = '→';
                }
            }
        } else if from_nl.row < to_nl.row {
            // Downward edge: source above, target below
            let from_cx = from_nl.x + from_nl.box_width / 2;
            let to_cx = to_nl.x + to_nl.box_width / 2;
            let from_bot_y = row_y(from_nl.row) + 2; // bottom of source box
            let to_top_y = row_y(to_nl.row); // top of target box

            if from_cx == to_cx {
                // Straight vertical
                for y in (from_bot_y + 1)..to_top_y {
                    if y < total_height && from_cx < width {
                        if grid[y][from_cx] == ' ' {
                            grid[y][from_cx] = '│';
                        }
                    }
                }
                if to_top_y > 0 && to_top_y - 1 < total_height && to_cx < width {
                    grid[to_top_y - 1][to_cx] = '▼';
                }
            } else {
                // L-shaped: go down from source, then horizontal, then down to target
                let turn_y = from_bot_y + 1;

                // Vertical from source down to turn
                if turn_y < total_height && from_cx < width {
                    grid[turn_y][from_cx] = if from_cx < to_cx { '└' } else { '┘' };
                }

                // Horizontal from source column to target column
                let (left, right) = if from_cx < to_cx {
                    (from_cx + 1, to_cx)
                } else {
                    (to_cx, from_cx.saturating_sub(1))
                };
                for j in left..right {
                    if turn_y < total_height && j < width && grid[turn_y][j] == ' ' {
                        grid[turn_y][j] = '─';
                    }
                }

                // Vertical from turn down to target
                if from_cx < to_cx && turn_y < total_height && to_cx < width {
                    grid[turn_y][to_cx] = '┐';
                } else if turn_y < total_height && to_cx < width {
                    grid[turn_y][to_cx] = '┌';
                }

                for y in (turn_y + 1)..to_top_y {
                    if y < total_height && to_cx < width && grid[y][to_cx] == ' ' {
                        grid[y][to_cx] = '│';
                    }
                }

                if to_top_y > 0 && to_top_y - 1 < total_height && to_cx < width {
                    grid[to_top_y - 1][to_cx] = '▼';
                }
            }
        } else {
            // Upward edge: source below, target above (feeder pattern)
            let from_right = from_nl.x + from_nl.box_width;
            let to_cx = to_nl.x + to_nl.box_width / 2;
            let y = row_y(from_nl.row) + 1; // mid line of source

            // Draw horizontal arrow from source's right to target's center column
            if from_right <= to_cx {
                for j in from_right..to_cx {
                    if j < width && grid[y][j] == ' ' {
                        grid[y][j] = '─';
                    }
                }
                if to_cx < width {
                    grid[y][to_cx] = '→';
                }
                // Draw upward connection from arrow to target's bottom
                let to_bot_y = row_y(to_nl.row) + 2;
                for vy in (to_bot_y + 1)..y {
                    if vy < total_height && to_cx < width && grid[vy][to_cx] == ' ' {
                        grid[vy][to_cx] = '│';
                    }
                }
                // Arrow meets the bottom of the target box — mark with ▲ if there's room
                if to_bot_y + 1 < total_height && to_cx < width && grid[to_bot_y + 1][to_cx] == ' ' {
                    // already drawing │ there, that's fine
                }
            }
        }
    }

    // Convert grid to StyledText lines, trimming trailing spaces
    grid.iter()
        .map(|row| {
            let s: String = row.iter().collect::<String>().trim_end().to_string();
            StyledText::plain(s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chain() {
        let input = "[A] -> [B] -> [C]";
        let diagram = parse_diagram(input);
        assert_eq!(diagram.nodes.len(), 3);
        assert_eq!(diagram.edges.len(), 2);
        assert_eq!(diagram.nodes[0].label, "A");
        assert_eq!(diagram.nodes[1].label, "B");
        assert_eq!(diagram.nodes[2].label, "C");
    }

    #[test]
    fn test_parse_multiple_chains() {
        let input = "[A] -> [B]\n[B] -> [C]\n[A] -> [D]";
        let diagram = parse_diagram(input);
        assert_eq!(diagram.nodes.len(), 4);
        assert_eq!(diagram.edges.len(), 3);
    }

    #[test]
    fn test_dedup_nodes() {
        let input = "[A] -> [B]\n[B] -> [C]";
        let diagram = parse_diagram(input);
        assert_eq!(diagram.nodes.len(), 3);
    }

    #[test]
    fn test_layout_fits_in_width() {
        let input = "[A] -> [B] -> [C]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 80);
        assert_eq!(layout.nodes[0].row, 0);
        assert_eq!(layout.nodes[1].row, 0);
        assert_eq!(layout.nodes[2].row, 0);
        assert_eq!(layout.nodes[0].x, 0);
        assert!(layout.nodes[1].x > layout.nodes[0].x);
        assert!(layout.nodes[2].x > layout.nodes[1].x);
    }

    #[test]
    fn test_layout_wraps_on_narrow_width() {
        let input = "[Alpha] -> [Beta] -> [Gamma] -> [Delta] -> [Epsilon] -> [Zeta]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 60);
        let max_row = layout.nodes.iter().map(|n| n.row).max().unwrap();
        assert!(max_row > 0, "chain should wrap to multiple rows");
    }

    #[test]
    fn test_layout_per_node_widths() {
        let input = "[A] -> [LongLabel]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 80);
        assert_eq!(layout.nodes[0].box_width, 5);
        assert_eq!(layout.nodes[1].box_width, 13);
    }

    #[test]
    fn test_render_produces_lines() {
        let input = "[Input] -> [Process] -> [Output]";
        let lines = render_diagram(input, 80);
        assert!(!lines.is_empty());
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_render_fits_width() {
        let input = "[Raw Data] -> [Preprocessing] -> [Feature Eng.] -> [Training] -> [Evaluation] -> [Deployment]";
        let lines = render_diagram(input, 80);
        for line in &lines {
            let text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
            let char_count = text.chars().count();
            assert!(
                char_count <= 80,
                "line exceeds width 80: ({} chars) {:?}",
                char_count,
                text
            );
        }
    }

    #[test]
    fn test_converging_inputs() {
        let input = "[A] -> [Shared] -> [Out]\n[B] -> [Shared]\n[C] -> [Shared]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 80);
        // Shared should be on the main chain (row 0)
        assert_eq!(layout.nodes[1].row, 0, "Shared should be on row 0");
        // B and C should be on later rows
        assert!(layout.nodes[3].row > 0, "B should be on a later row");
        assert!(layout.nodes[4].row > 0, "C should be on a later row");
    }

    #[test]
    fn test_diverging_outputs() {
        let input = "[In] -> [Hub] -> [Out1]\n[Hub] -> [Out2]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 80);
        // Out1 and Out2 should both be placed (not orphaned)
        assert!(layout.nodes.len() == 4);
        // Out2 should be placed near Out1
        let out1_row = layout.nodes[2].row;
        let out2_row = layout.nodes[3].row;
        assert!(
            out1_row == out2_row || (out2_row as isize - out1_row as isize).unsigned_abs() <= 1,
            "Out2 should be near Out1"
        );
    }
}
