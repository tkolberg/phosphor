# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Phosphor is a terminal slide deck presentation tool built on **ratatui**. Slides are authored in Markdown and rendered natively in the terminal — including charts, block diagrams, and raster images (via halfblock characters). A key differentiator is a **presenter notes system** that syncs notes to a separate terminal window via Unix domain socket.

## Build & Development

```bash
cargo build                      # build
cargo run -- <slides.md>         # run presenter
cargo run -- <slides.md> notes --socket <path>  # run notes viewer
cargo run -- <slides.md> test --slide 3         # test fire: render slide 3 to stdout
cargo run -- <slides.md> test --slide 3 --widths 40,80,120 --height 24  # test at specific sizes
cargo test                       # all tests
cargo test <test_name>           # single test
cargo clippy                     # lint
cargo fmt                        # format
cargo fmt -- --check             # format check (CI)
```

### Two-Terminal Notes Workflow
```bash
# Terminal 1 (presenter): starts server, prints socket path to stderr
cargo run -- slides.md
# Terminal 2 (notes viewer): connects to socket
cargo run -- slides.md notes --socket /tmp/phosphor-notes-<pid>.sock
```

## Architecture

### Rendering Pipeline

```
Markdown → comrak → SlideElement enum → Lower trait → Vec<RenderOp> → RenderEngine → ratatui Frame
```

1. **Parse** (`src/parse.rs`): comrak parses markdown; top-level nodes become `SlideElement` variants. Slides split on `---` (thematic breaks). Notes extracted from `<!-- notes: ... -->` comments. Chunk boundaries at `<!-- chunk -->` markers.
2. **Lower** (`src/render/lower.rs`): `Lower` trait converts each `SlideElement` into `Vec<RenderOp>`. The `LowerContext` carries terminal width, height, and theme reference. Lowering happens every frame with current terminal dimensions, so resize triggers immediate re-rendering.
3. **Render** (`src/render/engine.rs`): `RenderEngine` walks ops and draws into a ratatui `Frame`. Manages a `Vec<WindowRect>` stack for nested margin/layout composition.

### Layout System

Window rectangle stack pattern — `PushWindowRect` narrows the drawable area, `PopWindowRect` restores it. Alignment (left/center/right) is resolved per-rect. Theme-defined slide margins set the outermost rect.

### Presenter Notes Protocol

Newline-delimited JSON over Unix domain socket (`src/notes/protocol.rs`). Messages: `SlideChanged { index, visible_chunks }` and `Quit`. Presenter is the server (`NotesServer`); notes viewer connects as client (`NotesClient`). Client uses a 50ms read timeout to avoid blocking the TUI event loop.

### Theming

Built-in Catppuccin-style default theme. Custom themes via `--theme <path.yaml>` or front matter `theme:` field. Supports:
- `palette:` map with named colors (hex `#rrggbb` or color names)
- `palette:<name>` references in style values
- Per-element styles: `heading`, `code`, `blockquote`, etc.
- Slide-level `bg`, `fg`, and `margin`

### Front Matter

Optional YAML front matter between `---` delimiters at document start:
```yaml
---
title: My Talk
author: Name
theme: path/to/theme.yaml
---
```
Theme path is relative to the slide file's directory. CLI `--theme` flag overrides front matter.

### Content Types

- **Charts** (`src/chart.rs`): Fenced code blocks with `chart` language. YAML spec (type, file, title, color) + CSV data. Bar charts use ratatui `BarChart`; line charts use `Chart` with `Dataset`. Height scales to 60% of available terminal height.
- **Diagrams** (`src/diagram.rs`): Fenced code blocks with `diagram` language. `[Box] -> [Box]` DSL parsed into a directed graph. 3-phase layout (spine placement, feeder stacking, sibling proximity) rendered to box-drawing characters on a 2D grid. Output is `StyledText` lines fed through `RenderText` ops.
- **Images** (`src/halfblock.rs`): `![alt](path.png)` rendered using halfblock characters (▀ with fg=top pixel, bg=bottom pixel — 2 vertical pixels per cell). Uses the `image` crate for decode/resize. Images scale to fill available terminal space; no Kitty/Sixel protocol dependency.
- **Word wrapping** (`src/render/lower.rs`): `wrap_styled_text()` preserves styled segments across line breaks. Applied to paragraphs, headings, lists (with hanging indent), and blockquotes.

### Test Fire Mode

`src/testfire.rs` — renders a single slide (all chunks) into a ratatui `TestBackend` at specified terminal sizes and dumps the buffer to stdout. Reports overflow when content height exceeds available space. No TUI or alternate screen.

### Key Modules

| Module | Responsibility |
|--------|---------------|
| `parse` | Markdown → `SlideElement` via comrak, slide splitting, chunk/notes extraction |
| `render/ops` | `RenderOp` enum, `Alignment`, `Margin` |
| `render/lower` | `Lower` trait, `LowerContext`, element → ops conversion, word wrapping |
| `render/engine` | `RenderEngine`, `WindowRect` stack, styled text / chart / image drawing |
| `slide` | `Presentation`, `Slide`, `SlideChunk` data model |
| `elements` | `SlideElement` enum, `StyledText`, `TextSegment`, `SegmentStyle` |
| `chart` | Chart spec types, CSV loading (bar and line data) |
| `diagram` | Diagram DSL parser, graph layout, box-drawing renderer |
| `halfblock` | Raster image → halfblock character lines |
| `testfire` | Single-slide stdout renderer for debugging |
| `theme` | YAML theme types, loader, palette color resolution |
| `metadata` | YAML front matter extraction |
| `notes` | Socket protocol, `NotesServer`, `NotesClient` |
| `input` | `Action` enum, key → action mapping |
| `app` | Presenter TUI event loop |
| `notes_app` | Notes viewer TUI event loop with elapsed timer |

## Design Principles

- **RenderOp as universal intermediate**: all content lowers to `RenderOp`; nothing renders directly to the frame.
- **Ratatui for I/O, not layout**: phosphor manages layout via the rect stack; ratatui handles terminal output and styled text.
- **Stateless render passes**: given a slide index and visible_chunks count, rendering is fully deterministic.
- **Size-responsive**: all content (text, charts, diagrams, images) adapts to terminal dimensions. Lowering runs every frame with current width/height, so resize is immediate.
