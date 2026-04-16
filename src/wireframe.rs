use std::sync::OnceLock;

use ratatui::style::Color;

use crate::braille::BrailleCanvas;

/// Cached detector model — built once, reused every frame.
static DETECTOR_MODEL: OnceLock<WireframeModel> = OnceLock::new();

/// A point in 3D space.
#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Rotate around the Y axis (azimuth).
    pub fn rotate_y(self, angle_rad: f64) -> Self {
        let (s, c) = angle_rad.sin_cos();
        Self {
            x: self.x * c + self.z * s,
            y: self.y,
            z: -self.x * s + self.z * c,
        }
    }

    /// Rotate around the X axis (elevation).
    pub fn rotate_x(self, angle_rad: f64) -> Self {
        let (s, c) = angle_rad.sin_cos();
        Self {
            x: self.x,
            y: self.y * c - self.z * s,
            z: self.y * s + self.z * c,
        }
    }
}

/// A colored edge in 3D space.
#[derive(Debug, Clone)]
pub struct Edge {
    pub a: Vec3,
    pub b: Vec3,
    pub color: Color,
}

/// A wireframe model: a collection of colored edges.
#[derive(Debug, Clone)]
pub struct WireframeModel {
    pub edges: Vec<Edge>,
}

/// Spec parsed from a `wireframe` code block.
#[derive(Debug, Clone)]
pub struct WireframeSpec {
    pub model: String,
    /// Azimuth angle in degrees.
    pub azimuth: f64,
    /// Elevation angle in degrees.
    pub elevation: f64,
    /// Continuous rotation speed in degrees/second (0 = static).
    pub spin: f64,
    /// Animate muon particles through the detector.
    pub particles: bool,
}

impl Default for WireframeSpec {
    fn default() -> Self {
        Self {
            model: "detector".to_string(),
            azimuth: 35.0,
            elevation: 20.0,
            spin: 0.0,
            particles: false,
        }
    }
}

/// Parse a wireframe spec from YAML-like key: value lines.
pub fn parse_wireframe_spec(input: &str) -> WireframeSpec {
    let mut spec = WireframeSpec::default();
    for line in input.lines() {
        let line = line.trim();
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "model" => spec.model = val.to_string(),
                "azimuth" => {
                    if let Ok(v) = val.parse() {
                        spec.azimuth = v;
                    }
                }
                "elevation" => {
                    if let Ok(v) = val.parse() {
                        spec.elevation = v;
                    }
                }
                "spin" => {
                    if let Ok(v) = val.parse() {
                        spec.spin = v;
                    }
                }
                "particles" => {
                    spec.particles = val == "true" || val == "on" || val == "yes";
                }
                "rotate" => {
                    // "az,el" shorthand
                    let parts: Vec<&str> = val.split(',').collect();
                    if let Some(az) = parts.first().and_then(|s| s.trim().parse().ok()) {
                        spec.azimuth = az;
                    }
                    if let Some(el) = parts.get(1).and_then(|s| s.trim().parse().ok()) {
                        spec.elevation = el;
                    }
                }
                _ => {}
            }
        }
    }
    spec
}

/// Build the DiRAC detector geometry as a wireframe model.
///
/// Matches the Geant4 DetectorConstruction: a single-tower sampling
/// calorimeter built along the +z axis.
///
///   EM section:  6 layers × (2mm Pb absorber + 0.3mm Si sensor), 3×3 grid, 5mm pitch
///   HAD section: 4 layers × (40mm Steel absorber + 3mm scintillator), 3×3 grid, 30mm pitch
///
/// Dimensions are in mm, then normalized so the tower fits in [-1, 1].
pub fn build_detector() -> WireframeModel {
    let mut edges = Vec::new();

    // Geant4 constants (mm)
    let n_em: usize = 6;
    let n_had: usize = 4;
    let cells_x: usize = 3;
    let cells_y: usize = 3;
    let em_absorber = 2.0;
    let had_absorber = 40.0;
    let em_pitch = 5.0;
    let had_pitch = 30.0;
    let si_thickness = 0.3;
    let scint_thickness = 3.0;

    // Compute total tower extent
    let em_layer_t = em_absorber + si_thickness;
    let had_layer_t = had_absorber + scint_thickness;
    let total_z = (n_em as f64) * em_layer_t + (n_had as f64) * had_layer_t;
    let em_xy = (cells_x.max(cells_y) as f64) * em_pitch;
    let had_xy = (cells_x.max(cells_y) as f64) * had_pitch;
    let max_xy = em_xy.max(had_xy);

    // Normalize: map to [-1, 1] range, centered on tower midpoint
    let scale = 1.0 / (total_z.max(max_xy) * 0.5);
    let z_offset = total_z / 2.0; // center the tower on z=0

    let em_color = Color::Rgb(255, 165, 0);    // orange
    let si_color = Color::Rgb(100, 149, 237);   // cornflower blue
    let had_color = Color::Rgb(147, 112, 219);  // medium purple
    let scint_color = Color::Rgb(0, 200, 120);  // green

    let mut z_cursor = 0.0_f64;

    // EM section
    for layer in 0..n_em {
        let half_xy = (em_xy / 2.0) * scale;

        // Absorber slab
        let z0 = (z_cursor - z_offset) * scale;
        z_cursor += em_absorber;
        let z1 = (z_cursor - z_offset) * scale;
        add_box(&mut edges, half_xy, half_xy, z0, z1, em_color);

        // Sensor grid: individual cells
        let cell_half = (em_pitch / 2.0) * scale;
        let sz0 = (z_cursor - z_offset) * scale;
        z_cursor += si_thickness;
        let sz1 = (z_cursor - z_offset) * scale;

        for iy in 0..cells_y {
            for ix in 0..cells_x {
                let cx = ((ix as f64) - (cells_x as f64 - 1.0) / 2.0) * em_pitch * scale;
                let cy = ((iy as f64) - (cells_y as f64 - 1.0) / 2.0) * em_pitch * scale;
                add_box_offset(
                    &mut edges,
                    cx - cell_half, cx + cell_half,
                    cy - cell_half, cy + cell_half,
                    sz0, sz1,
                    si_color,
                );
            }
        }
    }

    let em_z_end = z_cursor;

    // HAD section
    for layer in 0..n_had {
        let half_xy = (had_xy / 2.0) * scale;

        // Absorber slab
        let z0 = (z_cursor - z_offset) * scale;
        z_cursor += had_absorber;
        let z1 = (z_cursor - z_offset) * scale;
        add_box(&mut edges, half_xy, half_xy, z0, z1, had_color);

        // Scintillator tiles
        let cell_half = (had_pitch / 2.0) * scale;
        let sz0 = (z_cursor - z_offset) * scale;
        z_cursor += scint_thickness;
        let sz1 = (z_cursor - z_offset) * scale;

        for iy in 0..cells_y {
            for ix in 0..cells_x {
                let cx = ((ix as f64) - (cells_x as f64 - 1.0) / 2.0) * had_pitch * scale;
                let cy = ((iy as f64) - (cells_y as f64 - 1.0) / 2.0) * had_pitch * scale;
                add_box_offset(
                    &mut edges,
                    cx - cell_half, cx + cell_half,
                    cy - cell_half, cy + cell_half,
                    sz0, sz1,
                    scint_color,
                );
            }
        }
    }

    // Section boundary marker: a line along the EM/HAD interface
    let boundary_z = (em_z_end - z_offset) * scale;
    let max_half = (max_xy / 2.0) * scale;
    let boundary_color = Color::Rgb(255, 80, 80); // red
    // Cross at the EM/HAD boundary
    edges.push(Edge {
        a: Vec3::new(-max_half, -max_half, boundary_z),
        b: Vec3::new(max_half, max_half, boundary_z),
        color: boundary_color,
    });
    edges.push(Edge {
        a: Vec3::new(-max_half, max_half, boundary_z),
        b: Vec3::new(max_half, -max_half, boundary_z),
        color: boundary_color,
    });

    WireframeModel { edges }
}

/// Add axis-aligned box edges (centered at x=0, y=0).
fn add_box(edges: &mut Vec<Edge>, half_x: f64, half_y: f64, z0: f64, z1: f64, color: Color) {
    add_box_offset(edges, -half_x, half_x, -half_y, half_y, z0, z1, color);
}

/// Add axis-aligned box edges with explicit x/y bounds.
fn add_box_offset(
    edges: &mut Vec<Edge>,
    x0: f64, x1: f64,
    y0: f64, y1: f64,
    z0: f64, z1: f64,
    color: Color,
) {
    let corners_front = [
        Vec3::new(x0, y0, z0),
        Vec3::new(x1, y0, z0),
        Vec3::new(x1, y1, z0),
        Vec3::new(x0, y1, z0),
    ];
    let corners_back = [
        Vec3::new(x0, y0, z1),
        Vec3::new(x1, y0, z1),
        Vec3::new(x1, y1, z1),
        Vec3::new(x0, y1, z1),
    ];

    // Front face
    for i in 0..4 {
        edges.push(Edge { a: corners_front[i], b: corners_front[(i + 1) % 4], color });
    }
    // Back face
    for i in 0..4 {
        edges.push(Edge { a: corners_back[i], b: corners_back[(i + 1) % 4], color });
    }
    // Connecting edges
    for i in 0..4 {
        edges.push(Edge { a: corners_front[i], b: corners_back[i], color });
    }
}

/// Attenuate an RGB color by a brightness factor (0.0 = black, 1.0 = full).
fn fade_color(color: Color, brightness: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f64 * brightness) as u8,
            (g as f64 * brightness) as u8,
            (b as f64 * brightness) as u8,
        ),
        other => other,
    }
}

/// Perspective-project a 3D point to 2D screen coordinates.
/// Camera is at (0, 0, -dist) looking toward origin.
/// Returns (screen_x, screen_y) in normalized [-1, 1] space, or None if behind camera.
fn project(point: Vec3, camera_dist: f64, fov: f64) -> Option<(f64, f64)> {
    let z = point.z + camera_dist;
    if z <= 0.1 {
        return None;
    }
    let scale = fov / z;
    Some((point.x * scale, point.y * scale))
}

/// Render a wireframe model onto a BrailleCanvas.
pub fn render_wireframe(
    spec: &WireframeSpec,
    term_cols: u16,
    term_rows: u16,
) -> Vec<ratatui::text::Line<'static>> {
    let model = match spec.model.as_str() {
        "detector" => DETECTOR_MODEL.get_or_init(build_detector),
        _ => DETECTOR_MODEL.get_or_init(build_detector),
    };

    let mut canvas = BrailleCanvas::new(term_cols, term_rows);
    let pw = canvas.width as f64;
    let ph = canvas.height as f64;

    // Time-based rotation offset
    let spin_offset = if spec.spin != 0.0 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        (spec.spin * secs).to_radians()
    } else {
        0.0
    };

    let az = spec.azimuth.to_radians() + spin_offset;
    let el = spec.elevation.to_radians();

    let camera_dist = 3.2;
    let fov = 2.5;

    // Center of the canvas in pixel space
    let cx = pw / 2.0;
    let cy = ph / 2.0;

    // Scale factor: map normalized coords to pixel space
    // Use the smaller dimension to maintain aspect ratio
    // Account for braille aspect ratio: cells are ~2:1 tall
    let aspect_correction = 2.0; // braille pixels are roughly 2x taller than wide
    let scale = (pw.min(ph * aspect_correction)) * 0.48;

    // Pre-rotate all edges and compute depth info
    struct RotatedEdge {
        a: Vec3,
        b: Vec3,
        avg_z: f64,
        color: Color,
    }

    let mut rotated: Vec<RotatedEdge> = model
        .edges
        .iter()
        .map(|edge| {
            let a = edge.a.rotate_y(az).rotate_x(el);
            let b = edge.b.rotate_y(az).rotate_x(el);
            let avg_z = (a.z + b.z) / 2.0;
            RotatedEdge { a, b, avg_z, color: edge.color }
        })
        .collect();

    // Sort farthest first (most positive z) so nearest edges draw last and overwrite
    rotated.sort_by(|a, b| b.avg_z.partial_cmp(&a.avg_z).unwrap());

    let z_max = rotated.first().map(|e| e.avg_z).unwrap_or(0.0);
    let z_min = rotated.last().map(|e| e.avg_z).unwrap_or(0.0);
    let z_range = (z_max - z_min).max(0.001);

    for edge in &rotated {
        let pa = project(edge.a, camera_dist, fov);
        let pb = project(edge.b, camera_dist, fov);

        if let (Some((ax, ay)), Some((bx, by))) = (pa, pb) {
            let sx0 = (cx + ax * scale) as isize;
            let sy0 = (cy - ay * scale) as isize;
            let sx1 = (cx + bx * scale) as isize;
            let sy1 = (cy - by * scale) as isize;

            // Depth fade: closer edges brighter, farther edges dimmer
            let depth = (edge.avg_z - z_min) / z_range; // 0 = nearest, 1 = farthest
            let brightness = 1.0 - depth * 0.75;
            let faded = fade_color(edge.color, brightness);

            canvas.line_colored(sx0, sy0, sx1, sy1, faded);
        }
    }

    // Animate muon particles if enabled
    if spec.particles {
        draw_particles(&mut canvas, az, el, camera_dist, fov, cx, cy, scale);
    }

    canvas.render()
}

/// Deterministic muon particle animation.
///
/// Matches PrimaryGeneratorAction: cone mode, point source at z=-50mm,
/// cone subtending the HAD section (half-width 45mm at HAD midpoint 99.8mm).
fn draw_particles(
    canvas: &mut BrailleCanvas,
    az: f64,
    el: f64,
    camera_dist: f64,
    fov: f64,
    cx: f64,
    cy: f64,
    scale: f64,
) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    // Detector geometry (mm) — same as build_detector
    let n_em: usize = 6;
    let n_had: usize = 4;
    let em_absorber = 2.0_f64;
    let had_absorber = 40.0_f64;
    let si_thickness = 0.3_f64;
    let scint_thickness = 3.0_f64;
    let em_layer_t = em_absorber + si_thickness;
    let had_layer_t = had_absorber + scint_thickness;
    let total_z = (n_em as f64) * em_layer_t + (n_had as f64) * had_layer_t;
    let max_xy = 90.0_f64; // HAD extent: 3 * 30mm
    let norm_scale = 1.0 / (total_z.max(max_xy) * 0.5);
    let z_offset = total_z / 2.0;

    // Beam parameters from PrimaryGeneratorAction
    let source_z_mm = -50.0_f64;
    let tower_half_width = 45.0_f64;
    let had_mid_z = 99.8_f64;
    let theta_max = (tower_half_width / (50.0 + had_mid_z)).atan();

    // Particle timing (leisurely pace for presentation)
    let spawn_interval = 2.0;  // seconds between muons
    let travel_time = 6.0;     // seconds to cross the full detector
    let trail_frac = 0.15;     // trail length as fraction of total path

    // z range in mm: from source to beyond detector exit
    let z_start_mm = source_z_mm;
    let z_end_mm = total_z + 20.0; // exit buffer
    let path_len = z_end_mm - z_start_mm;

    // Simple hash for deterministic randomness per particle
    let hash = |seed: u64| -> f64 {
        let x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (x >> 33) as f64 / (1u64 << 31) as f64
    };

    let muon_color_head = Color::Rgb(255, 255, 255);
    let muon_color_mid = Color::Rgb(255, 255, 100);
    let muon_color_tail = Color::Rgb(255, 120, 40);

    // Check recent particle slots (travel_time / spawn_interval + buffer)
    let max_slots = ((travel_time / spawn_interval) as f64).ceil() as u64 + 2;
    for i in 0..max_slots {
        let slot_time = (now / spawn_interval).floor() as u64 - i;
        let birth = slot_time as f64 * spawn_interval;
        let age = now - birth;

        if age < 0.0 || age > travel_time {
            continue;
        }

        let progress = age / travel_time; // 0..1 through detector

        // Deterministic direction within cone (uniform in cos theta)
        let h1 = hash(slot_time * 3 + 0);
        let h2 = hash(slot_time * 3 + 1);
        let cos_min = theta_max.cos();
        let cos_theta = 1.0 - h1 * (1.0 - cos_min);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let phi = h2 * 2.0 * std::f64::consts::PI;

        let dx = sin_theta * phi.cos();
        let dy = sin_theta * phi.sin();
        let dz = cos_theta;

        // Current z position along beam axis
        let head_z_mm = z_start_mm + progress * path_len;
        let tail_z_mm = head_z_mm - trail_frac * path_len;

        // Compute 3D positions (start from source point, travel along direction)
        let t_head = (head_z_mm - source_z_mm) / dz;
        let t_tail = (tail_z_mm - source_z_mm) / dz;

        // Convert mm to normalized coords
        let to_norm = |t: f64| -> Vec3 {
            let x_mm = dx * t;
            let y_mm = dy * t;
            let z_mm = source_z_mm + dz * t;
            Vec3::new(
                x_mm * norm_scale,
                y_mm * norm_scale,
                (z_mm - z_offset) * norm_scale,
            )
        };

        // Draw trail in 3 segments with color gradient (tail → mid → head)
        let segments = [
            (t_tail, t_tail + (t_head - t_tail) * 0.33, muon_color_tail),
            (t_tail + (t_head - t_tail) * 0.33, t_tail + (t_head - t_tail) * 0.66, muon_color_mid),
            (t_tail + (t_head - t_tail) * 0.66, t_head, muon_color_head),
        ];

        for (t0, t1, color) in segments {
            let p0 = to_norm(t0).rotate_y(az).rotate_x(el);
            let p1 = to_norm(t1).rotate_y(az).rotate_x(el);

            let proj0 = project(p0, camera_dist, fov);
            let proj1 = project(p1, camera_dist, fov);

            if let (Some((x0, y0)), Some((x1, y1))) = (proj0, proj1) {
                let sx0 = (cx + x0 * scale) as isize;
                let sy0 = (cy - y0 * scale) as isize;
                let sx1 = (cx + x1 * scale) as isize;
                let sy1 = (cy - y1 * scale) as isize;
                canvas.line_colored(sx0, sy0, sx1, sy1, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spec_defaults() {
        let spec = parse_wireframe_spec("");
        assert_eq!(spec.model, "detector");
        assert!((spec.azimuth - 35.0).abs() < 0.01);
        assert!((spec.elevation - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_spec_rotate() {
        let spec = parse_wireframe_spec("model: detector\nrotate: 45,30");
        assert!((spec.azimuth - 45.0).abs() < 0.01);
        assert!((spec.elevation - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_build_detector_has_edges() {
        let model = build_detector();
        // Should have a reasonable number of edges
        assert!(model.edges.len() > 100);
    }

    #[test]
    fn test_render_produces_lines() {
        let spec = WireframeSpec::default();
        let lines = render_wireframe(&spec, 40, 20);
        assert_eq!(lines.len(), 20);
    }

    #[test]
    fn test_rotation_preserves_distance() {
        let p = Vec3::new(1.0, 0.0, 0.0);
        let r = p.rotate_y(0.5).rotate_x(0.3);
        let dist = (r.x * r.x + r.y * r.y + r.z * r.z).sqrt();
        assert!((dist - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_projection() {
        // Point at origin should project to center
        let p = project(Vec3::new(0.0, 0.0, 0.0), 4.0, 2.5);
        assert!(p.is_some());
        let (x, y) = p.unwrap();
        assert!((x).abs() < 1e-10);
        assert!((y).abs() < 1e-10);
    }

    #[test]
    fn test_behind_camera() {
        // Point behind camera should return None
        let p = project(Vec3::new(0.0, 0.0, -5.0), 4.0, 2.5);
        assert!(p.is_none());
    }
}
