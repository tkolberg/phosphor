---
title: Chart Demo
theme: themes/synthwave.yaml
---

# Chart Rendering Demo

Five charts to stress-test the rendering.

---

# 10-Model Comparison (Bar)

```chart
type: bar
file: data/model_comparison.csv
title: ImageNet Top-1 Accuracy (%)
color: cyan
```

---

# Training Loss Curve (Line)

```chart
type: line
file: data/training_loss.csv
title: Training Loss — 25 Epochs
x_label: Epoch
y_label: Loss
color: green
```

---

# HGCAL Channel Counts (Bar)

```chart
type: bar
file: data/channel_counts.csv
title: Readout Channels per Subsystem
color: magenta
```

---

# Signal Distribution (Line)

```chart
type: line
file: data/signal_background.csv
title: Invariant Mass Distribution
x_label: m(GeV)
y_label: Events/5GeV
color: yellow
```

---

# DAQ Throughput (Line)

```chart
type: line
file: data/daq_throughput.csv
title: DAQ Throughput vs Buffer Depth
x_label: Buffer Depth
y_label: Throughput (%)
color: red
```

---

# HGCAL Module Specifications

| Module | Channels | Layers | Technology | Power (W) |
|--------|:--------:|-------:|------------|:---------:|
| CE-E LD | 192 | 26 | Si 300um | 4.2 |
| CE-E HD | 432 | 26 | Si 120um | 8.7 |
| CE-H Si | 192 | 12 | Si 300um | 3.8 |
| CE-H Scint | 48 | 12 | SiPM+Scint | 2.1 |
| HGCROC v3 | 72 | — | 130nm CMOS | 0.9 |
| ECON-T | 12 | — | 65nm CMOS | 1.4 |
| lpGBT | 2 | — | 65nm CMOS | 0.8 |

---

# Collaboration Timeline

| Milestone | Target | Status | Lead |
|-----------|--------|--------|------|
| TDR Approved | 2018-Q4 | Complete | CERN |
| Sensor Procurement | 2023-Q2 | Complete | HPK/FBK |
| Module Assembly | 2025-Q1 | In Progress | FSU/FNAL |
| Pre-series Cassettes | 2025-Q3 | Pending | Multiple |
| Installation Phase 1 | 2026-Q2 | Planned | CMS |
| Commissioning | 2027-Q1 | Planned | CMS |
| HL-LHC Data Taking | 2029 | Planned | CMS |

---

# Semantic Highlighting

<!-- chunk -->

The {key: marginal cost} of exploration has fundamentally changed.

<!-- chunk -->

James C. Scott called it {jargon: mêtis} — the practical local knowledge that makes complex systems actually work.

<!-- chunk -->

- A {definition: BCR} (Baseline Change Request) used to take 4 hours to draft
- The {warning: displacement question}: reinvestment vs. attrition
- {emphasis: The bottleneck moves from implementation to taste}

---

# Comparison of Approaches

| | Manual | Script-Assisted | AI-Augmented |
|---|---|---|---|
| BCR Draft Time | 4 hours | 2 hours | 20 min |
| Code Review | 1-2 days | 1 day | 2-4 hours |
| Format Translation | 30 min/page | 10 min/page | 1 min/page |
| Vendor Email | 45 min | 30 min | 5 min |
| Test Stand Script | 1-2 days | 4 hours | 30 min |
