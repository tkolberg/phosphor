---
title: Phosphor Demo
author: Ted
---

# Phosphor

A terminal presentation tool built on ratatui.

---

# Architecture

<!-- chunk -->

- **Operation-based rendering** pipeline

<!-- chunk -->

- **Window rectangle stack** for layout

<!-- chunk -->

- **Socket-synced presenter notes** to a second terminal

<!-- notes: Explain how the architecture borrows from presenterm's design. The key insight is that RenderOp decouples content from terminal I/O. -->

---

# How It Works

Slides are written in Markdown:

```markdown
# My Slide

- Point one
- Point two
```

Then rendered in your terminal with full color theming.

<!-- notes: Demo the live rendering here. Show how the theme changes colors. -->

---

# Incremental Reveals

Use `<!-- chunk -->` comments to reveal content step by step.

<!-- chunk -->

Each press of the spacebar reveals the next chunk.

<!-- chunk -->

> "Simplicity is the ultimate sophistication."

<!-- notes: This is a good time to show the chunk mechanism. Press space to reveal each point. -->

---

# Model Accuracy

```chart
type: bar
file: data/accuracy.csv
title: ImageNet Top-1 Accuracy
color: cyan
```

<!-- notes: Bar chart rendered natively with ratatui BarChart widget. Data loaded from CSV. -->

---

# Training Loss

```chart
type: line
file: data/loss.csv
title: Training Loss Over Epochs
x_label: Epoch
y_label: Loss
color: green
```

<!-- notes: Line chart using braille markers for high-resolution rendering in the terminal. -->

---

# ML Pipeline

```diagram
[Raw Data] -> [Preprocessing] -> [Feature Eng.]
[Feature Eng.] -> [Training] -> [Evaluation]
[Evaluation] -> [Deployment]
```

<!-- notes: Block diagram rendered with box-drawing characters. The DSL auto-deduplicates nodes by name. -->

---

# The End

Thanks for watching!

*Built with* **phosphor** *and* `ratatui`.
