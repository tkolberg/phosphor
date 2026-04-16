use std::path::Path;

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChartSpec {
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    pub file: String,
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    Bar,
    Line,
}

#[derive(Debug, Clone)]
pub struct BarData {
    pub labels: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct LineData {
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub enum ChartData {
    Bar(BarData),
    Line(LineData),
}

pub fn parse_chart_spec(yaml: &str) -> Result<ChartSpec> {
    serde_yaml::from_str(yaml).wrap_err("Failed to parse chart spec")
}

pub fn load_chart_data(spec: &ChartSpec, base_dir: &Path) -> Result<ChartData> {
    let path = base_dir.join(&spec.file);
    match spec.chart_type {
        ChartType::Bar => {
            let data = load_bar_data(&path)?;
            Ok(ChartData::Bar(data))
        }
        ChartType::Line => {
            let data = load_line_data(&path)?;
            Ok(ChartData::Line(data))
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

fn load_line_data(path: &Path) -> Result<LineData> {
    let mut reader = csv::Reader::from_path(path)
        .wrap_err_with(|| format!("Failed to open CSV {:?}", path))?;

    let mut points = Vec::new();

    for result in reader.records() {
        let record = result.wrap_err("Failed to read CSV record")?;
        let x: f64 = record
            .get(0)
            .ok_or_else(|| eyre!("Missing x column"))?
            .trim()
            .parse()
            .wrap_err("Failed to parse x as number")?;
        let y: f64 = record
            .get(1)
            .ok_or_else(|| eyre!("Missing y column"))?
            .trim()
            .parse()
            .wrap_err("Failed to parse y as number")?;
        points.push((x, y));
    }

    Ok(LineData { points })
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
    fn test_load_line_data() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "x,y\n1.0,2.0\n2.0,4.0\n3.0,3.0").unwrap();

        let data = load_line_data(&csv_path).unwrap();
        assert_eq!(data.points, vec![(1.0, 2.0), (2.0, 4.0), (3.0, 3.0)]);
    }
}
