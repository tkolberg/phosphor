use std::path::{Path, PathBuf};

use comrak::{Arena, Options, parse_document};
use comrak::nodes::{AstNode, NodeValue};

use crate::chart;
use crate::elements::{*, TableAlignment};
use crate::slide::*;

/// Parse a markdown string into a Presentation.
///
/// Slides are separated by `---` (thematic breaks).
/// Notes are extracted from HTML comments matching `<!-- notes: ... -->`.
/// `base_dir` is used to resolve relative file paths in chart specs.
pub fn parse_presentation(markdown: &str, base_dir: &Path) -> Presentation {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    let root = parse_document(&arena, markdown, &options);

    let mut slides: Vec<Slide> = Vec::new();
    let mut current_elements: Vec<SlideElement> = Vec::new();
    let mut current_notes: Option<String> = None;

    for node in root.children() {
        let ast = node.data.borrow();
        match &ast.value {
            NodeValue::ThematicBreak => {
                // Slide boundary
                slides.push(build_slide(
                    std::mem::take(&mut current_elements),
                    current_notes.take(),
                ));
            }
            NodeValue::HtmlBlock(html) => {
                if is_chunk_marker(&html.literal) {
                    current_elements.push(SlideElement::ChunkBreak);
                } else if let Some(notes) = extract_notes(&html.literal) {
                    current_notes = Some(notes);
                }
            }
            _ => {
                if let Some(element) = convert_node(node, base_dir) {
                    current_elements.push(expand_highlights(element));
                }
            }
        }
    }

    // Don't forget the last slide
    if !current_elements.is_empty() {
        slides.push(build_slide(current_elements, current_notes.take()));
    }

    let title = extract_title(&slides);

    Presentation {
        slides,
        metadata: PresentationMetadata { title },
    }
}

fn build_slide(elements: Vec<SlideElement>, notes: Option<String>) -> Slide {
    // Split elements into chunks on ChunkBreak markers.
    // A ChunkBreak always starts a new chunk (even if empty), so that
    // camera-only advances on wireframe slides work without dummy content.
    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut saw_break = false;

    for element in elements {
        if matches!(element, SlideElement::ChunkBreak) {
            chunks.push(SlideChunk {
                elements: std::mem::take(&mut current_chunk),
            });
            saw_break = true;
        } else {
            current_chunk.push(element);
        }
    }
    if !current_chunk.is_empty() || saw_break {
        chunks.push(SlideChunk {
            elements: current_chunk,
        });
    }
    // Ensure at least one chunk
    if chunks.is_empty() {
        chunks.push(SlideChunk {
            elements: Vec::new(),
        });
    }

    Slide { chunks, notes }
}

fn is_chunk_marker(html: &str) -> bool {
    let trimmed = html.trim();
    trimmed == "<!-- chunk -->"
}

fn extract_title(slides: &[Slide]) -> Option<String> {
    let first_slide = slides.first()?;
    let first_chunk = first_slide.chunks.first()?;
    let first_element = first_chunk.elements.first()?;
    match first_element {
        SlideElement::Heading { level: 1, text } => {
            let title: String = text.segments.iter().map(|s| s.text.as_str()).collect();
            Some(title)
        }
        _ => None,
    }
}

fn extract_notes(html: &str) -> Option<String> {
    let trimmed = html.trim();
    let inner = trimmed.strip_prefix("<!--")?.strip_suffix("-->")?;
    let inner = inner.trim();
    let body = inner.strip_prefix("notes:")?.trim();
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

fn convert_node<'a>(node: &'a AstNode<'a>, base_dir: &Path) -> Option<SlideElement> {
    let ast = node.data.borrow();
    match &ast.value {
        NodeValue::Heading(heading) => {
            let text = collect_inline_text(node);
            Some(SlideElement::Heading {
                level: heading.level,
                text,
            })
        }
        NodeValue::Paragraph => {
            // Check if this paragraph is a standalone image
            if let Some(image_element) = try_extract_image(node, base_dir) {
                return Some(image_element);
            }
            let text = collect_inline_text(node);
            if text.is_empty() {
                None
            } else {
                Some(SlideElement::Paragraph { text })
            }
        }
        NodeValue::CodeBlock(code_block) => {
            match code_block.info.as_str() {
                "chart" => {
                    match chart::parse_chart_spec(&code_block.literal) {
                        Ok(spec) => Some(SlideElement::Chart {
                            spec,
                            base_dir: base_dir.to_path_buf(),
                        }),
                        Err(_) => Some(SlideElement::Code {
                            language: Some("chart".to_string()),
                            code: code_block.literal.trim_end().to_string(),
                        }),
                    }
                }
                "diagram" => {
                    Some(SlideElement::Diagram {
                        source: code_block.literal.trim_end().to_string(),
                    })
                }
                "wireframe" => {
                    Some(SlideElement::Wireframe {
                        source: code_block.literal.trim_end().to_string(),
                    })
                }
                _ => {
                    let language = if code_block.info.is_empty() {
                        None
                    } else {
                        Some(code_block.info.clone())
                    };
                    Some(SlideElement::Code {
                        language,
                        code: code_block.literal.trim_end().to_string(),
                    })
                }
            }
        }
        NodeValue::List(list) => {
            let ordered = list.list_type == comrak::nodes::ListType::Ordered;
            let start = list.start;
            let items = collect_list_items(node, 0);
            Some(SlideElement::List {
                items,
                ordered,
                start,
            })
        }
        NodeValue::BlockQuote => {
            let text = collect_block_quote_text(node);
            Some(SlideElement::BlockQuote { text })
        }
        NodeValue::Table(table) => {
            let alignments: Vec<TableAlignment> = table
                .alignments
                .iter()
                .map(|a| match a {
                    comrak::nodes::TableAlignment::None => TableAlignment::None,
                    comrak::nodes::TableAlignment::Left => TableAlignment::Left,
                    comrak::nodes::TableAlignment::Center => TableAlignment::Center,
                    comrak::nodes::TableAlignment::Right => TableAlignment::Right,
                })
                .collect();
            let (headers, rows) = collect_table_rows(node);
            Some(SlideElement::Table {
                headers,
                rows,
                alignments,
            })
        }
        _ => None,
    }
}

/// If a paragraph contains only a single image node (optionally with whitespace),
/// extract it as a standalone Image element.
fn try_extract_image<'a>(node: &'a AstNode<'a>, base_dir: &Path) -> Option<SlideElement> {
    let children: Vec<_> = node.children().collect();

    // Find the single Image child (allow surrounding whitespace text nodes)
    let mut image_node = None;
    for child in &children {
        let ast = child.data.borrow();
        match &ast.value {
            NodeValue::Image(_) => {
                if image_node.is_some() {
                    return None; // multiple images
                }
                image_node = Some(*child);
            }
            NodeValue::Text(t) if t.trim().is_empty() => {}
            NodeValue::SoftBreak | NodeValue::LineBreak => {}
            _ => return None, // non-image content
        }
    }

    let img_node = image_node?;
    let img_ast = img_node.data.borrow();
    if let NodeValue::Image(ref link) = img_ast.value {
        // Collect alt text from children
        let alt: String = img_node
            .children()
            .filter_map(|c| {
                let a = c.data.borrow();
                if let NodeValue::Text(t) = &a.value {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Some(SlideElement::Image {
            path: link.url.clone(),
            alt,
            base_dir: base_dir.to_path_buf(),
        })
    } else {
        None
    }
}

fn collect_inline_text<'a>(node: &'a AstNode<'a>) -> StyledText {
    let mut segments = Vec::new();
    collect_inline_segments(node, &SegmentStyle::default(), &mut segments);
    StyledText { segments }
}

fn collect_inline_segments<'a>(
    node: &'a AstNode<'a>,
    parent_style: &SegmentStyle,
    segments: &mut Vec<TextSegment>,
) {
    for child in node.children() {
        let ast = child.data.borrow();
        match &ast.value {
            NodeValue::Text(text) => {
                segments.push(TextSegment {
                    text: text.clone(),
                    style: parent_style.clone(),
                });
            }
            NodeValue::Code(code) => {
                segments.push(TextSegment {
                    text: code.literal.clone(),
                    style: SegmentStyle {
                        code: true,
                        ..parent_style.clone()
                    },
                });
            }
            NodeValue::Strong => {
                let style = SegmentStyle {
                    bold: true,
                    ..parent_style.clone()
                };
                collect_inline_segments(child, &style, segments);
            }
            NodeValue::Emph => {
                let style = SegmentStyle {
                    italic: true,
                    ..parent_style.clone()
                };
                collect_inline_segments(child, &style, segments);
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => {
                segments.push(TextSegment {
                    text: " ".to_string(),
                    style: parent_style.clone(),
                });
            }
            _ => {
                // Recurse into other inline containers
                collect_inline_segments(child, parent_style, segments);
            }
        }
    }
}

fn collect_list_items<'a>(node: &'a AstNode<'a>, depth: u8) -> Vec<ListItem> {
    let mut items = Vec::new();
    for child in node.children() {
        let ast = child.data.borrow();
        if matches!(ast.value, NodeValue::Item(_)) {
            // Collect text from the first paragraph child of the item
            let mut text = StyledText { segments: vec![] };
            for item_child in child.children() {
                let item_ast = item_child.data.borrow();
                match &item_ast.value {
                    NodeValue::Paragraph => {
                        text = collect_inline_text(item_child);
                    }
                    NodeValue::List(_) => {
                        // Nested list
                        items.push(ListItem {
                            depth,
                            text: std::mem::replace(
                                &mut text,
                                StyledText { segments: vec![] },
                            ),
                        });
                        items.extend(collect_list_items(item_child, depth + 1));
                        continue;
                    }
                    _ => {}
                }
            }
            if !text.is_empty() {
                items.push(ListItem { depth, text });
            }
        }
    }
    items
}

/// Post-process a SlideElement to expand `{class: text}` highlight markup in all StyledText fields.
fn expand_highlights(element: SlideElement) -> SlideElement {
    match element {
        SlideElement::Heading { level, text } => SlideElement::Heading {
            level,
            text: expand_highlights_in_text(text),
        },
        SlideElement::Paragraph { text } => SlideElement::Paragraph {
            text: expand_highlights_in_text(text),
        },
        SlideElement::BlockQuote { text } => SlideElement::BlockQuote {
            text: expand_highlights_in_text(text),
        },
        SlideElement::List {
            items,
            ordered,
            start,
        } => SlideElement::List {
            items: items
                .into_iter()
                .map(|item| ListItem {
                    depth: item.depth,
                    text: expand_highlights_in_text(item.text),
                })
                .collect(),
            ordered,
            start,
        },
        // Table cells, code, chart, diagram — no highlight expansion needed
        other => other,
    }
}

/// Expand `{class: text}` patterns in a StyledText.
fn expand_highlights_in_text(styled: StyledText) -> StyledText {
    let mut new_segments = Vec::new();
    for seg in styled.segments {
        expand_highlights_in_segment(seg, &mut new_segments);
    }
    StyledText {
        segments: new_segments,
    }
}

/// Split a single text segment on `{class: text}` patterns, producing
/// plain segments and highlighted segments.
fn expand_highlights_in_segment(seg: TextSegment, out: &mut Vec<TextSegment>) {
    let text = &seg.text;
    let mut rest = text.as_str();

    while let Some(open) = rest.find('{') {
        // Look for the closing brace
        let after_open = &rest[open + 1..];
        if let Some(close) = after_open.find('}') {
            let inner = &after_open[..close];
            // Must contain ": " to be a highlight (not just any braces)
            if let Some(colon) = inner.find(": ") {
                let class = inner[..colon].trim();
                let content = inner[colon + 2..].trim();

                // Validate class name: alphanumeric + hyphens/underscores only
                if !class.is_empty()
                    && class
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    // Emit the text before the highlight
                    let before = &rest[..open];
                    if !before.is_empty() {
                        out.push(TextSegment {
                            text: before.to_string(),
                            style: seg.style.clone(),
                        });
                    }

                    // Emit the highlighted segment
                    out.push(TextSegment {
                        text: content.to_string(),
                        style: SegmentStyle {
                            highlight: Some(class.to_string()),
                            ..seg.style.clone()
                        },
                    });

                    // Continue after the closing brace
                    rest = &after_open[close + 1..];
                    continue;
                }
            }
        }

        // Not a valid highlight pattern — emit up through the `{` and continue
        out.push(TextSegment {
            text: rest[..open + 1].to_string(),
            style: seg.style.clone(),
        });
        rest = &rest[open + 1..];
    }

    // Emit any remaining text
    if !rest.is_empty() {
        out.push(TextSegment {
            text: rest.to_string(),
            style: seg.style.clone(),
        });
    }
}

fn collect_block_quote_text<'a>(node: &'a AstNode<'a>) -> StyledText {
    let mut segments = Vec::new();
    for child in node.children() {
        let ast = child.data.borrow();
        if matches!(ast.value, NodeValue::Paragraph) {
            collect_inline_segments(child, &SegmentStyle::default(), &mut segments);
        }
    }
    StyledText { segments }
}

fn collect_table_rows<'a>(node: &'a AstNode<'a>) -> (Vec<StyledText>, Vec<Vec<StyledText>>) {
    let mut headers = Vec::new();
    let mut rows = Vec::new();

    for row_node in node.children() {
        let row_ast = row_node.data.borrow();
        if let NodeValue::TableRow(is_header) = row_ast.value {
            let cells: Vec<StyledText> = row_node
                .children()
                .map(|cell_node| collect_inline_text(cell_node))
                .collect();
            if is_header {
                headers = cells;
            } else {
                rows.push(cells);
            }
        }
    }

    (headers, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_slide_splitting() {
        let md = "# Slide One\n\nHello world\n\n---\n\n# Slide Two\n\nGoodbye\n";
        let pres = parse_presentation(md, Path::new("."));
        assert_eq!(pres.slides.len(), 2);
        assert_eq!(pres.metadata.title, Some("Slide One".to_string()));
    }

    #[test]
    fn test_three_slides() {
        let md = "# A\n\n---\n\n# B\n\n---\n\n# C\n";
        let pres = parse_presentation(md, Path::new("."));
        assert_eq!(pres.slides.len(), 3);
    }

    #[test]
    fn test_notes_extraction() {
        let md = "# Slide\n\nContent\n\n<!-- notes: These are my notes -->\n\n---\n\n# Next\n";
        let pres = parse_presentation(md, Path::new("."));
        assert_eq!(pres.slides[0].notes, Some("These are my notes".to_string()));
        assert_eq!(pres.slides[1].notes, None);
    }

    #[test]
    fn test_heading_levels() {
        let md = "# H1\n\n## H2\n\n### H3\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        assert!(matches!(&elements[0], SlideElement::Heading { level: 1, .. }));
        assert!(matches!(&elements[1], SlideElement::Heading { level: 2, .. }));
        assert!(matches!(&elements[2], SlideElement::Heading { level: 3, .. }));
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {}\n```\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::Code { language, code } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(code, "fn main() {}");
            }
            _ => panic!("Expected Code element"),
        }
    }

    #[test]
    fn test_list() {
        let md = "- Item A\n- Item B\n- Item C\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::List {
                items,
                ordered,
                ..
            } => {
                assert!(!ordered);
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected List element"),
        }
    }

    #[test]
    fn test_chunk_splitting() {
        let md = "# Title\n\n<!-- chunk -->\n\nFirst point\n\n<!-- chunk -->\n\nSecond point\n";
        let pres = parse_presentation(md, Path::new("."));
        assert_eq!(pres.slides[0].chunks.len(), 3);
        assert!(matches!(
            &pres.slides[0].chunks[0].elements[0],
            SlideElement::Heading { .. }
        ));
        assert!(matches!(
            &pres.slides[0].chunks[1].elements[0],
            SlideElement::Paragraph { .. }
        ));
    }

    #[test]
    fn test_inline_styles() {
        let md = "This has **bold** and *italic* and `code` text\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::Paragraph { text } => {
                assert!(text.segments.len() > 1);
                // Find the bold segment
                let bold = text.segments.iter().find(|s| s.style.bold).unwrap();
                assert_eq!(bold.text, "bold");
                // Find the italic segment
                let italic = text.segments.iter().find(|s| s.style.italic).unwrap();
                assert_eq!(italic.text, "italic");
                // Find the code segment
                let code = text.segments.iter().find(|s| s.style.code).unwrap();
                assert_eq!(code.text, "code");
            }
            _ => panic!("Expected Paragraph element"),
        }
    }

    #[test]
    fn test_table_parsing() {
        let md = "| Model | Accuracy | Params |\n|-------|----------|--------|\n| ResNet | 94.2 | 25M |\n| VGG | 91.8 | 138M |\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::Table { headers, rows, alignments } => {
                assert_eq!(headers.len(), 3);
                assert_eq!(rows.len(), 2);
                assert_eq!(alignments.len(), 3);
                let h0: String = headers[0].segments.iter().map(|s| s.text.as_str()).collect();
                assert_eq!(h0, "Model");
            }
            _ => panic!("Expected Table element"),
        }
    }

    #[test]
    fn test_highlight_expansion() {
        let md = "The {key: marginal cost} of exploration has changed.\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::Paragraph { text } => {
                assert!(text.segments.len() >= 3);
                let highlighted = text.segments.iter().find(|s| s.style.highlight.is_some()).unwrap();
                assert_eq!(highlighted.text, "marginal cost");
                assert_eq!(highlighted.style.highlight.as_deref(), Some("key"));
            }
            _ => panic!("Expected Paragraph element"),
        }
    }

    #[test]
    fn test_highlight_preserves_plain_braces() {
        let md = "Use `{some_dict}` in code and {key: real highlight} here.\n";
        let pres = parse_presentation(md, Path::new("."));
        let elements = &pres.slides[0].chunks[0].elements;
        match &elements[0] {
            SlideElement::Paragraph { text } => {
                let highlighted = text.segments.iter().find(|s| s.style.highlight.is_some()).unwrap();
                assert_eq!(highlighted.text, "real highlight");
            }
            _ => panic!("Expected Paragraph element"),
        }
    }
}
