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
    // │ Label │ = border + padding + label + padding + border
    label.len() + 4
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

    for &root in &roots {
        // Collect the chain by following first unplaced successor
        let mut chain: Vec<usize> = Vec::new();
        let mut current = root;
        loop {
            if node_layouts[current].is_some() {
                break;
            }
            chain.push(current);
            if let Some(succs) = successors.get(&current) {
                if let Some(&next) = succs.iter().find(|&&s| node_layouts[s].is_none()) {
                    current = next;
                    continue;
                }
            }
            break;
        }

        if chain.is_empty() {
            continue;
        }

        // Place the chain, wrapping when width is exceeded
        let mut x: usize = 0;
        for &node_idx in &chain {
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
    }

    // Place any remaining nodes
    for i in 0..diagram.nodes.len() {
        if node_layouts[i].is_none() {
            node_layouts[i] = Some(NodeLayout {
                x: 0,
                row: current_row,
                box_width: box_widths[i],
            });
            current_row += 1;
        }
    }

    Layout {
        nodes: node_layouts.into_iter().map(|n| n.unwrap()).collect(),
        num_rows: current_row,
    }
}

/// Render a diagram to styled text lines using box-drawing characters.
pub fn render_diagram(input: &str, available_width: u16) -> Vec<StyledText> {
    let diagram = parse_diagram(input);
    if diagram.nodes.is_empty() {
        return vec![StyledText::plain("[empty diagram]")];
    }

    let layout = layout_diagram(&diagram, available_width);

    let line_width = layout
        .nodes
        .iter()
        .map(|n| n.x + n.box_width)
        .max()
        .unwrap_or(0);

    // Group nodes by row, sorted by x
    let mut row_nodes: Vec<Vec<usize>> = vec![Vec::new(); layout.num_rows];
    for (idx, nl) in layout.nodes.iter().enumerate() {
        row_nodes[nl.row].push(idx);
    }
    for rn in &mut row_nodes {
        rn.sort_by_key(|&idx| layout.nodes[idx].x);
    }

    let edge_set: Vec<(usize, usize)> = diagram.edges.iter().map(|e| (e.from, e.to)).collect();

    let trim = |chars: &[char]| -> String {
        chars.iter().collect::<String>().trim_end().to_string()
    };

    let mut lines: Vec<StyledText> = Vec::new();

    for row in 0..layout.num_rows {
        let nodes = &row_nodes[row];
        let mut top = vec![' '; line_width];
        let mut mid = vec![' '; line_width];
        let mut bot = vec![' '; line_width];

        for (i, &node_idx) in nodes.iter().enumerate() {
            let nl = &layout.nodes[node_idx];
            let label = &diagram.nodes[node_idx].label;
            let inner_w = nl.box_width - 2;

            // ┌───┐
            top[nl.x] = '┌';
            for j in 1..=inner_w {
                top[nl.x + j] = '─';
            }
            top[nl.x + nl.box_width - 1] = '┐';

            // │ Label │
            mid[nl.x] = '│';
            let padded = format!("{:^width$}", label, width = inner_w);
            for (j, ch) in padded.chars().enumerate() {
                mid[nl.x + 1 + j] = ch;
            }
            mid[nl.x + nl.box_width - 1] = '│';

            // └───┘
            bot[nl.x] = '└';
            for j in 1..=inner_w {
                bot[nl.x + j] = '─';
            }
            bot[nl.x + nl.box_width - 1] = '┘';

            // Horizontal arrow to next node in row (if edge exists)
            if i + 1 < nodes.len() {
                let next_idx = nodes[i + 1];
                if edge_set.iter().any(|&(f, t)| f == node_idx && t == next_idx) {
                    let start = nl.x + nl.box_width;
                    let end = layout.nodes[next_idx].x;
                    if end > start + 1 {
                        for j in start..end - 1 {
                            mid[j] = '─';
                        }
                        mid[end - 1] = '→';
                    } else if end > start {
                        mid[start] = '→';
                    }
                }
            }
        }

        lines.push(StyledText::plain(trim(&top)));
        lines.push(StyledText::plain(trim(&mid)));
        lines.push(StyledText::plain(trim(&bot)));

        // Cross-row connectors: edges from this row to a later row
        let cross_edges: Vec<(usize, usize)> = edge_set
            .iter()
            .filter(|&&(from, to)| {
                layout.nodes[from].row == row && layout.nodes[to].row > row
            })
            .copied()
            .collect();

        if !cross_edges.is_empty() {
            // Vertical bars descending from source node centers
            let mut vline = vec![' '; line_width];
            for &(from, _) in &cross_edges {
                let nl = &layout.nodes[from];
                let cx = nl.x + nl.box_width / 2;
                if cx < line_width {
                    vline[cx] = '│';
                }
            }
            lines.push(StyledText::plain(trim(&vline)));

            // L-shaped connectors for edges where source and target aren't vertically aligned
            for &(from, to) in &cross_edges {
                let from_cx = layout.nodes[from].x + layout.nodes[from].box_width / 2;
                let to_cx = layout.nodes[to].x + layout.nodes[to].box_width / 2;

                if from_cx != to_cx {
                    let mut lline = vec![' '; line_width];
                    if from_cx > to_cx {
                        // Wrap left: ┌──────┘
                        lline[to_cx] = '┌';
                        for j in to_cx + 1..from_cx {
                            lline[j] = '─';
                        }
                        lline[from_cx] = '┘';
                    } else {
                        // Wrap right: └──────┐
                        lline[from_cx] = '└';
                        for j in from_cx + 1..to_cx {
                            lline[j] = '─';
                        }
                        lline[to_cx] = '┐';
                    }
                    lines.push(StyledText::plain(trim(&lline)));
                }
            }

            // Arrow tips at target node centers
            let mut aline = vec![' '; line_width];
            for &(_, to) in &cross_edges {
                let nl = &layout.nodes[to];
                let cx = nl.x + nl.box_width / 2;
                if cx < line_width {
                    aline[cx] = '▼';
                }
            }
            lines.push(StyledText::plain(trim(&aline)));
        }
    }

    lines
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
        // All on row 0, each at increasing x
        assert_eq!(layout.nodes[0].row, 0);
        assert_eq!(layout.nodes[1].row, 0);
        assert_eq!(layout.nodes[2].row, 0);
        assert_eq!(layout.nodes[0].x, 0);
        assert!(layout.nodes[1].x > layout.nodes[0].x);
        assert!(layout.nodes[2].x > layout.nodes[1].x);
    }

    #[test]
    fn test_layout_wraps_on_narrow_width() {
        // 6 nodes in a chain should wrap at narrow widths
        let input = "[Alpha] -> [Beta] -> [Gamma] -> [Delta] -> [Epsilon] -> [Zeta]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 60);
        // Not all nodes should fit on row 0
        let max_row = layout.nodes.iter().map(|n| n.row).max().unwrap();
        assert!(max_row > 0, "chain should wrap to multiple rows");
    }

    #[test]
    fn test_layout_per_node_widths() {
        let input = "[A] -> [LongLabel]";
        let diagram = parse_diagram(input);
        let layout = layout_diagram(&diagram, 80);
        // A has box_width 5, LongLabel has box_width 13
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
}
