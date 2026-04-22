use std::path::Path;

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeriesSpec {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChartSpec {
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    pub file: String,
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub color: Option<String>,
    pub series: Option<Vec<SeriesSpec>>,
    pub bins: Option<usize>,
    pub log_y: Option<bool>,
    pub x_min: Option<f64>,
    pub x_max: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    Bar,
    Line,
    Scatter,
    Histogram,
}

#[derive(Debug, Clone)]
pub struct BarData {
    pub labels: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub color: Option<String>,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct MultiSeriesData {
    pub series: Vec<Series>,
}

#[derive(Debug, Clone)]
pub struct HistogramData {
    pub series: Vec<HistogramSeries>,
}

#[derive(Debug, Clone)]
pub struct HistogramSeries {
    pub name: String,
    pub color: Option<String>,
    pub bin_edges: Vec<f64>,
    pub counts: Vec<f64>,
}

#[derive(Debug, Clone)]
pub enum ChartData {
    Bar(BarData),
    MultiSeries(MultiSeriesData),
    Histogram(HistogramData),
}

pub fn parse_chart_spec(yaml: &str) -> Result<ChartSpec> {
    serde_yaml::from_str(yaml).wrap_err("Failed to parse chart spec")
}

const DEFAULT_SERIES_COLORS: &[&str] = &["cyan", "magenta", "yellow", "green", "red", "blue"];

pub fn load_chart_data(spec: &ChartSpec, base_dir: &Path) -> Result<ChartData> {
    let path = base_dir.join(&spec.file);
    match spec.chart_type {
        ChartType::Bar => {
            let data = load_bar_data(&path)?;
            Ok(ChartData::Bar(data))
        }
        ChartType::Line | ChartType::Scatter => {
            let data = load_multi_series_data(spec, &path)?;
            Ok(ChartData::MultiSeries(data))
        }
        ChartType::Histogram => {
            let data = load_histogram_data(spec, &path)?;
            Ok(ChartData::Histogram(data))
        }
    }
}

fn load_bar_data(path: &Path) -> Result<BarData> {
    let mut reader = csv::Reader::from_path(path)
        .wrap_err_with(|| format!("Failed to open CSV {:?}", path))?;

    let mut labels = Vec::new();
    let mut values = Vec::new();

    for result in reader.records() {
        let record = result.wrap_err("Failed to read CSV record")?;
        let label = record
            .get(0)
            .ok_or_else(|| eyre!("Missing label column"))?
            .to_string();
        let value: f64 = record
            .get(1)
            .ok_or_else(|| eyre!("Missing value column"))?
            .trim()
            .parse()
            .wrap_err("Failed to parse value as number")?;
        labels.push(label);
        values.push(value);
    }

    Ok(BarData { labels, values })
}

fn load_multi_series_data(spec: &ChartSpec, path: &Path) -> Result<MultiSeriesData> {
    let mut reader = csv::Reader::from_path(path)
        .wrap_err_with(|| format!("Failed to open CSV {:?}", path))?;

    let headers: Vec<String> = reader
        .headers()
        .wrap_err("Failed to read CSV headers")?
        .iter()
        .map(|h| h.to_string())
        .collect();

    let num_cols = headers.len();
    if num_cols < 2 {
        return Err(eyre!("CSV must have at least 2 columns (x + y)"));
    }

    let num_series = num_cols - 1;
    let mut all_points: Vec<Vec<(f64, f64)>> = vec![Vec::new(); num_series];

    for result in reader.records() {
        let record = result.wrap_err("Failed to read CSV record")?;
        let x: f64 = record
            .get(0)
            .ok_or_else(|| eyre!("Missing x column"))?
            .trim()
            .parse()
            .wrap_err("Failed to parse x as number")?;

        for i in 0..num_series {
            let val = record
                .get(i + 1)
                .ok_or_else(|| eyre!("Missing column {}", i + 1))?
                .trim();
            if val.is_empty() || val.eq_ignore_ascii_case("nan") {
                continue;
            }
            let y: f64 = val.parse().wrap_err_with(|| {
                format!("Failed to parse column {} as number", headers[i + 1])
            })?;
            if y.is_finite() {
                all_points[i].push((x, y));
            }
        }
    }

    let series_specs = spec.series.as_deref().unwrap_or(&[]);

    let series = all_points
        .into_iter()
        .enumerate()
        .map(|(i, points)| {
            let spec_entry = series_specs.get(i);
            let name = spec_entry
                .and_then(|s| s.name.clone())
                .unwrap_or_else(|| headers[i + 1].clone());
            let color = spec_entry
                .and_then(|s| s.color.clone())
                .or_else(|| Some(DEFAULT_SERIES_COLORS[i % DEFAULT_SERIES_COLORS.len()].to_string()));
            Series {
                name,
                color,
                points,
            }
        })
        .collect();

    Ok(MultiSeriesData { series })
}

fn load_histogram_data(spec: &ChartSpec, path: &Path) -> Result<HistogramData> {
    let mut reader = csv::Reader::from_path(path)
        .wrap_err_with(|| format!("Failed to open CSV {:?}", path))?;

    let headers: Vec<String> = reader
        .headers()
        .wrap_err("Failed to read CSV headers")?
        .iter()
        .map(|h| h.to_string())
        .collect();

    let num_cols = headers.len();

    // Detect format: pre-binned (bin_low, bin_high, count1, ...) vs raw values (val1, val2, ...)
    let is_prebinned = num_cols >= 3
        && (headers[0].to_lowercase().contains("bin") || headers[0].to_lowercase().contains("low"))
        && (headers[1].to_lowercase().contains("bin") || headers[1].to_lowercase().contains("high"));

    if is_prebinned {
        load_prebinned_histogram(&headers, &mut reader, spec)
    } else {
        load_raw_histogram(&headers, &mut reader, spec)
    }
}

fn load_prebinned_histogram(
    headers: &[String],
    reader: &mut csv::Reader<std::fs::File>,
    spec: &ChartSpec,
) -> Result<HistogramData> {
    let num_series = headers.len() - 2;
    let mut bin_lows: Vec<f64> = Vec::new();
    let mut bin_highs: Vec<f64> = Vec::new();
    let mut all_counts: Vec<Vec<f64>> = vec![Vec::new(); num_series];

    for result in reader.records() {
        let record = result.wrap_err("Failed to read CSV record")?;
        let lo: f64 = record.get(0).unwrap().trim().parse()?;
        let hi: f64 = record.get(1).unwrap().trim().parse()?;
        bin_lows.push(lo);
        bin_highs.push(hi);
        for i in 0..num_series {
            let v: f64 = record
                .get(i + 2)
                .ok_or_else(|| eyre!("Missing count column"))?
                .trim()
                .parse()?;
            all_counts[i].push(v);
        }
    }

    let series_specs = spec.series.as_deref().unwrap_or(&[]);

    let series = all_counts
        .into_iter()
        .enumerate()
        .map(|(i, counts)| {
            let mut edges = bin_lows.clone();
            if let Some(last_hi) = bin_highs.last() {
                edges.push(*last_hi);
            }
            let spec_entry = series_specs.get(i);
            let name = spec_entry
                .and_then(|s| s.name.clone())
                .unwrap_or_else(|| headers[i + 2].clone());
            let color = spec_entry
                .and_then(|s| s.color.clone())
                .or_else(|| Some(DEFAULT_SERIES_COLORS[i % DEFAULT_SERIES_COLORS.len()].to_string()));
            HistogramSeries {
                name,
                color,
                bin_edges: edges,
                counts,
            }
        })
        .collect();

    Ok(HistogramData { series })
}

fn load_raw_histogram(
    headers: &[String],
    reader: &mut csv::Reader<std::fs::File>,
    spec: &ChartSpec,
) -> Result<HistogramData> {
    let num_series = headers.len();
    let mut all_values: Vec<Vec<f64>> = vec![Vec::new(); num_series];

    for result in reader.records() {
        let record = result.wrap_err("Failed to read CSV record")?;
        for i in 0..num_series {
            let val = record
                .get(i)
                .ok_or_else(|| eyre!("Missing column {}", i))?
                .trim();
            if val.is_empty() || val.eq_ignore_ascii_case("nan") {
                continue;
            }
            let v: f64 = val.parse().wrap_err_with(|| {
                format!("Failed to parse column {} as number", headers[i])
            })?;
            if v.is_finite() {
                all_values[i].push(v);
            }
        }
    }

    let n_bins = spec.bins.unwrap_or(20);
    let series_specs = spec.series.as_deref().unwrap_or(&[]);

    // Find global min/max across all series for consistent binning
    let global_min = all_values
        .iter()
        .flat_map(|v| v.iter())
        .copied()
        .fold(f64::INFINITY, f64::min);
    let global_max = all_values
        .iter()
        .flat_map(|v| v.iter())
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    if !global_min.is_finite() || !global_max.is_finite() {
        return Err(eyre!("No finite values in histogram data"));
    }

    let range = global_max - global_min;
    let bin_width = if range == 0.0 {
        1.0
    } else {
        range / n_bins as f64
    };

    let bin_edges: Vec<f64> = (0..=n_bins)
        .map(|i| global_min + i as f64 * bin_width)
        .collect();

    let series = all_values
        .into_iter()
        .enumerate()
        .map(|(i, values)| {
            let mut counts = vec![0.0; n_bins];
            for v in &values {
                let bin = ((v - global_min) / bin_width).floor() as usize;
                let bin = bin.min(n_bins - 1);
                counts[bin] += 1.0;
            }
            let spec_entry = series_specs.get(i);
            let name = spec_entry
                .and_then(|s| s.name.clone())
                .unwrap_or_else(|| headers[i].clone());
            let color = spec_entry
                .and_then(|s| s.color.clone())
                .or_else(|| Some(DEFAULT_SERIES_COLORS[i % DEFAULT_SERIES_COLORS.len()].to_string()));
            HistogramSeries {
                name,
                color,
                bin_edges: bin_edges.clone(),
                counts,
            }
        })
        .collect();

    Ok(HistogramData { series })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_bar_spec() {
        let yaml = "type: bar\nfile: data.csv\ntitle: Accuracy\ncolor: cyan\n";
        let spec = parse_chart_spec(yaml).unwrap();
        assert_eq!(spec.chart_type, ChartType::Bar);
        assert_eq!(spec.file, "data.csv");
        assert_eq!(spec.title.as_deref(), Some("Accuracy"));
        assert_eq!(spec.color.as_deref(), Some("cyan"));
    }

    #[test]
    fn test_parse_line_spec() {
        let yaml = "type: line\nfile: loss.csv\nx_label: Epoch\ny_label: Loss\n";
        let spec = parse_chart_spec(yaml).unwrap();
        assert_eq!(spec.chart_type, ChartType::Line);
        assert_eq!(spec.x_label.as_deref(), Some("Epoch"));
    }

    #[test]
    fn test_parse_scatter_spec() {
        let yaml = "type: scatter\nfile: data.csv\ntitle: Gain Recovery\nseries:\n  - name: DiRAC\n    color: cyan\n  - name: Langaus\n    color: magenta\n";
        let spec = parse_chart_spec(yaml).unwrap();
        assert_eq!(spec.chart_type, ChartType::Scatter);
        assert_eq!(spec.series.as_ref().unwrap().len(), 2);
        assert_eq!(
            spec.series.as_ref().unwrap()[0].name.as_deref(),
            Some("DiRAC")
        );
    }

    #[test]
    fn test_parse_histogram_spec() {
        let yaml = "type: histogram\nfile: residuals.csv\nbins: 30\ntitle: Energy Residuals\n";
        let spec = parse_chart_spec(yaml).unwrap();
        assert_eq!(spec.chart_type, ChartType::Histogram);
        assert_eq!(spec.bins, Some(30));
    }

    #[test]
    fn test_load_bar_data() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "label,value\nA,10.5\nB,20.3\nC,15.0").unwrap();

        let data = load_bar_data(&csv_path).unwrap();
        assert_eq!(data.labels, vec!["A", "B", "C"]);
        assert_eq!(data.values, vec![10.5, 20.3, 15.0]);
    }

    #[test]
    fn test_load_multi_series() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "x,dirac,langaus\n1.0,0.9,0.7\n2.0,1.8,1.5\n3.0,2.7,2.0").unwrap();

        let spec = ChartSpec {
            chart_type: ChartType::Scatter,
            file: "test.csv".into(),
            title: None,
            x_label: None,
            y_label: None,
            color: None,
            series: None,
            bins: None,
            log_y: None,
            x_min: None,
            x_max: None,
        };
        let data = load_multi_series_data(&spec, &csv_path).unwrap();
        assert_eq!(data.series.len(), 2);
        assert_eq!(data.series[0].name, "dirac");
        assert_eq!(data.series[1].name, "langaus");
        assert_eq!(data.series[0].points.len(), 3);
    }

    #[test]
    fn test_load_single_series_line() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "x,y\n1.0,2.0\n2.0,4.0\n3.0,3.0").unwrap();

        let spec = ChartSpec {
            chart_type: ChartType::Line,
            file: "test.csv".into(),
            title: None,
            x_label: None,
            y_label: None,
            color: Some("cyan".into()),
            series: None,
            bins: None,
            log_y: None,
            x_min: None,
            x_max: None,
        };
        let data = load_multi_series_data(&spec, &csv_path).unwrap();
        assert_eq!(data.series.len(), 1);
        assert_eq!(data.series[0].points, vec![(1.0, 2.0), (2.0, 4.0), (3.0, 3.0)]);
    }

    #[test]
    fn test_raw_histogram_binning() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "values\n1.0\n1.5\n2.0\n2.5\n3.0\n3.5\n4.0\n4.5\n5.0\n5.5").unwrap();

        let spec = ChartSpec {
            chart_type: ChartType::Histogram,
            file: "test.csv".into(),
            title: None,
            x_label: None,
            y_label: None,
            color: None,
            series: None,
            bins: Some(5),
            log_y: None,
            x_min: None,
            x_max: None,
        };
        let data = load_histogram_data(&spec, &csv_path).unwrap();
        assert_eq!(data.series.len(), 1);
        assert_eq!(data.series[0].counts.len(), 5);
        assert_eq!(data.series[0].bin_edges.len(), 6);
        let total: f64 = data.series[0].counts.iter().sum();
        assert_eq!(total, 10.0);
    }

    #[test]
    fn test_prebinned_histogram() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "bin_low,bin_high,signal,background\n0,1,10,50\n1,2,25,40\n2,3,15,30").unwrap();

        let spec = ChartSpec {
            chart_type: ChartType::Histogram,
            file: "test.csv".into(),
            title: None,
            x_label: None,
            y_label: None,
            color: None,
            series: None,
            bins: None,
            log_y: None,
            x_min: None,
            x_max: None,
        };
        let data = load_histogram_data(&spec, &csv_path).unwrap();
        assert_eq!(data.series.len(), 2);
        assert_eq!(data.series[0].name, "signal");
        assert_eq!(data.series[1].name, "background");
        assert_eq!(data.series[0].counts, vec![10.0, 25.0, 15.0]);
        assert_eq!(data.series[0].bin_edges, vec![0.0, 1.0, 2.0, 3.0]);
    }
}
