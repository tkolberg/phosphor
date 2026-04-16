# Phosphor TODOs

## Bugs / Polish
- [ ] Word wrap can still clip on very narrow terminals with long unbreakable tokens (URLs, code spans)
- [ ] Image re-rendering on resize reloads from disk every frame — cache the decoded image, only re-render halfblocks on dimension change
- [ ] Line charts only scale vertically, not horizontally (axis labels don't adapt to width)
- [ ] Cleanup compiler warnings (unused imports, dead code variants)
- [ ] Slides with dense content can still overflow vertically — no auto-scaling or scroll indicator

## Features
- [ ] README.md for the public repo (install instructions, screenshots, feature overview)
- [ ] `cargo install` support (publish to crates.io)
- [ ] Syntax highlighting for code blocks (tree-sitter or syntect)
- [ ] LaTeX / math rendering in tables and text
- [ ] Slide jump: type a number to go directly to a slide
- [ ] Slide overview / grid mode
- [ ] Hot reload: watch the markdown file and re-parse on change
- [ ] Export to PDF or HTML (static snapshot)
- [ ] Footer template customization (currently hardcoded layout)
- [ ] Custom key bindings via config

## Rendering
- [ ] Vertical centering option for slides with sparse content
- [ ] Font size awareness — detect cell dimensions for better image scaling decisions
- [ ] Diagram compact mode: single-line boxes instead of 3-line box-drawing for tight slides
- [ ] Better halfblock image quality — dithering, edge-aware downsampling
- [ ] Background image support (full-slide behind text)

## Talk: FSU HEP Seminar (talks branch)
- [ ] Replace placeholder DiRAC data with real numbers from diagnostic HTML files
- [ ] Section VIII ("What Collaborations Should Actually Do") is still thin
- [ ] Find better closing image for "Come With Me If You Want to Live" slide
- [ ] Dry run at target terminal size to check all slides fit
- [ ] Test presenter notes sync end-to-end in two-terminal setup
