use std::sync::{Mutex, OnceLock};

use ratatui::style::Color;

use crate::braille::BrailleCanvas;

/// Cached detector model — built once, reused every frame.
static DETECTOR_MODEL: OnceLock<WireframeModel> = OnceLock::new();

/// Camera animation state — tracks current keyframe target and transition timing.
static CAMERA_ANIM: Mutex<CameraAnimState> = Mutex::new(CameraAnimState {
    target_index: 0,
    transition_start: 0.0,
});

struct CameraAnimState {
    target_index: usize,
    transition_start: f64,
}

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

/// A colored edge in 3D space, tagged with its parent cell's bounding box.
#[derive(Debug, Clone)]
pub struct Edge {
    pub a: Vec3,
    pub b: Vec3,
    pub color: Color,
    /// Bounding box min of the cell this edge belongs to (model space).
    pub cell_min: Vec3,
    /// Bounding box max of the cell this edge belongs to (model space).
    pub cell_max: Vec3,
}

/// A wireframe model: a collection of colored edges.
#[derive(Debug, Clone)]
pub struct WireframeModel {
    pub edges: Vec<Edge>,
}

/// A camera keyframe: position and orientation for one step in a camera sequence.
#[derive(Debug, Clone)]
pub struct CameraKeyframe {
    pub distance: f64,
    pub fov: f64,
    pub azimuth: f64,
    pub elevation: f64,
    /// Look-at point offset in normalized model space.
    pub focus_x: f64,
    pub focus_y: f64,
    pub focus_z: f64,
}

impl Default for CameraKeyframe {
    fn default() -> Self {
        Self {
            distance: 3.2,
            fov: 2.5,
            azimuth: 35.0,
            elevation: 20.0,
            focus_x: 0.0,
            focus_y: 0.0,
            focus_z: 0.0,
        }
    }
}

impl CameraKeyframe {
    fn lerp(&self, other: &CameraKeyframe, t: f64) -> CameraKeyframe {
        let t = t.clamp(0.0, 1.0);
        CameraKeyframe {
            distance: self.distance + (other.distance - self.distance) * t,
            fov: self.fov + (other.fov - self.fov) * t,
            azimuth: self.azimuth + (other.azimuth - self.azimuth) * t,
            elevation: self.elevation + (other.elevation - self.elevation) * t,
            focus_x: self.focus_x + (other.focus_x - self.focus_x) * t,
            focus_y: self.focus_y + (other.focus_y - self.focus_y) * t,
            focus_z: self.focus_z + (other.focus_z - self.focus_z) * t,
        }
    }
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
    /// Camera keyframes for chunk-driven camera movement.
    /// Empty = use default single camera from azimuth/elevation/distance.
    pub cameras: Vec<CameraKeyframe>,
    /// Duration of camera transitions in seconds.
    pub camera_transition: f64,
}

impl Default for WireframeSpec {
    fn default() -> Self {
        Self {
            model: "detector".to_string(),
            azimuth: 35.0,
            elevation: 20.0,
            spin: 0.0,
            particles: false,
            cameras: Vec::new(),
            camera_transition: 2.0,
        }
    }
}

/// Parse a wireframe spec from YAML-like key: value lines.
///
/// Camera keyframes are specified as `camera:` lines with space-separated key=value pairs:
/// ```text
/// camera: distance=2.0 focus_z=-0.5 azimuth=25 elevation=15
/// camera: distance=3.2 azimuth=35 elevation=20
/// ```
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
                    let parts: Vec<&str> = val.split(',').collect();
                    if let Some(az) = parts.first().and_then(|s| s.trim().parse().ok()) {
                        spec.azimuth = az;
                    }
                    if let Some(el) = parts.get(1).and_then(|s| s.trim().parse().ok()) {
                        spec.elevation = el;
                    }
                }
                "camera" => {
                    spec.cameras.push(parse_camera_keyframe(val));
                }
                "camera_transition" => {
                    if let Ok(v) = val.parse() {
                        spec.camera_transition = v;
                    }
                }
                _ => {}
            }
        }
    }
    spec
}

/// Parse a camera keyframe from space-separated key=value pairs.
fn parse_camera_keyframe(input: &str) -> CameraKeyframe {
    let mut kf = CameraKeyframe::default();
    for token in input.split_whitespace() {
        if let Some((k, v)) = token.split_once('=') {
            let v: f64 = match v.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            match k {
                "distance" | "dist" => kf.distance = v,
                "fov" => kf.fov = v,
                "azimuth" | "az" => kf.azimuth = v,
                "elevation" | "el" => kf.elevation = v,
                "focus_x" | "fx" => kf.focus_x = v,
                "focus_y" | "fy" => kf.focus_y = v,
                "focus_z" | "fz" => kf.focus_z = v,
                _ => {}
            }
        }
    }
    kf
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
    // Cross at the EM/HAD boundary (no parent cell — use huge bounds so it never flashes)
    let no_cell_min = Vec3::new(-999.0, -999.0, -999.0);
    let no_cell_max = Vec3::new(-998.0, -998.0, -998.0);
    edges.push(Edge {
        a: Vec3::new(-max_half, -max_half, boundary_z),
        b: Vec3::new(max_half, max_half, boundary_z),
        color: boundary_color,
        cell_min: no_cell_min,
        cell_max: no_cell_max,
    });
    edges.push(Edge {
        a: Vec3::new(-max_half, max_half, boundary_z),
        b: Vec3::new(max_half, -max_half, boundary_z),
        color: boundary_color,
        cell_min: no_cell_min,
        cell_max: no_cell_max,
    });

    WireframeModel { edges }
}

/// Add axis-aligned box edges (centered at x=0, y=0).
/// These are absorber slabs — they get dummy cell bounds so they never flash.
fn add_box(edges: &mut Vec<Edge>, half_x: f64, half_y: f64, z0: f64, z1: f64, color: Color) {
    let no_cell_min = Vec3::new(-999.0, -999.0, -999.0);
    let no_cell_max = Vec3::new(-998.0, -998.0, -998.0);
    let corners_front = [
        Vec3::new(-half_x, -half_y, z0),
        Vec3::new(half_x, -half_y, z0),
        Vec3::new(half_x, half_y, z0),
        Vec3::new(-half_x, half_y, z0),
    ];
    let corners_back = [
        Vec3::new(-half_x, -half_y, z1),
        Vec3::new(half_x, -half_y, z1),
        Vec3::new(half_x, half_y, z1),
        Vec3::new(-half_x, half_y, z1),
    ];
    for i in 0..4 {
        edges.push(Edge { a: corners_front[i], b: corners_front[(i + 1) % 4], color, cell_min: no_cell_min, cell_max: no_cell_max });
    }
    for i in 0..4 {
        edges.push(Edge { a: corners_back[i], b: corners_back[(i + 1) % 4], color, cell_min: no_cell_min, cell_max: no_cell_max });
    }
    for i in 0..4 {
        edges.push(Edge { a: corners_front[i], b: corners_back[i], color, cell_min: no_cell_min, cell_max: no_cell_max });
    }
}

/// Add axis-aligned box edges with explicit x/y bounds.
fn add_box_offset(
    edges: &mut Vec<Edge>,
    x0: f64, x1: f64,
    y0: f64, y1: f64,
    z0: f64, z1: f64,
    color: Color,
) {
    let cell_min = Vec3::new(x0.min(x1), y0.min(y1), z0.min(z1));
    let cell_max = Vec3::new(x0.max(x1), y0.max(y1), z0.max(z1));

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
        edges.push(Edge { a: corners_front[i], b: corners_front[(i + 1) % 4], color, cell_min, cell_max });
    }
    // Back face
    for i in 0..4 {
        edges.push(Edge { a: corners_back[i], b: corners_back[(i + 1) % 4], color, cell_min, cell_max });
    }
    // Connecting edges
    for i in 0..4 {
        edges.push(Edge { a: corners_front[i], b: corners_back[i], color, cell_min, cell_max });
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

/// Smoothstep ease-in-out for camera transitions.
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Resolve the current camera parameters from the spec and animation state.
fn resolve_camera(spec: &WireframeSpec, camera_index: usize) -> CameraKeyframe {
    if spec.cameras.is_empty() {
        // No keyframes — use spec defaults
        return CameraKeyframe {
            distance: 3.2,
            fov: 2.5,
            azimuth: spec.azimuth,
            elevation: spec.elevation,
            focus_x: 0.0,
            focus_y: 0.0,
            focus_z: 0.0,
        };
    }

    let target = camera_index.min(spec.cameras.len().saturating_sub(1));
    let now = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    };

    let mut anim = CAMERA_ANIM.lock().unwrap();
    if anim.target_index != target {
        anim.target_index = target;
        anim.transition_start = now;
    }

    let elapsed = now - anim.transition_start;
    let duration = spec.camera_transition;
    let t = if duration > 0.0 {
        smoothstep((elapsed / duration).clamp(0.0, 1.0))
    } else {
        1.0
    };

    let target_kf = &spec.cameras[target];
    if target == 0 || t >= 1.0 {
        return target_kf.clone();
    }

    // Interpolate from previous keyframe
    let prev = &spec.cameras[target - 1];
    prev.lerp(target_kf, t)
}

/// Render a wireframe model onto a BrailleCanvas.
///
/// `camera_index` selects which camera keyframe to use (tied to visible_chunks).
pub fn render_wireframe(
    spec: &WireframeSpec,
    term_cols: u16,
    term_rows: u16,
    camera_index: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let model = match spec.model.as_str() {
        "detector" => DETECTOR_MODEL.get_or_init(build_detector),
        _ => DETECTOR_MODEL.get_or_init(build_detector),
    };

    let cam = resolve_camera(spec, camera_index);

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

    let az = cam.azimuth.to_radians() + spin_offset;
    let el = cam.elevation.to_radians();
    let camera_dist = cam.distance;
    let fov = cam.fov;
    let focus = Vec3::new(cam.focus_x, cam.focus_y, cam.focus_z);

    // Center of the canvas in pixel space
    let cx = pw / 2.0;
    let cy = ph / 2.0;

    // Scale factor: map normalized coords to pixel space
    let aspect_correction = 2.0;
    let scale = (pw.min(ph * aspect_correction)) * 0.48;

    // Compute active muon 3D positions in normalized model space (for detector hit flash)
    let muon_hits = if spec.particles {
        compute_muon_positions()
    } else {
        vec![]
    };

    // Pre-rotate all edges and compute depth info
    struct RotatedEdge {
        a: Vec3,
        b: Vec3,
        avg_z: f64,
        cell_min: Vec3,
        cell_max: Vec3,
        color: Color,
    }

    let mut rotated: Vec<RotatedEdge> = model
        .edges
        .iter()
        .map(|edge| {
            // Translate to focus point, then rotate
            let a = Vec3::new(edge.a.x - focus.x, edge.a.y - focus.y, edge.a.z - focus.z)
                .rotate_y(az).rotate_x(el);
            let b = Vec3::new(edge.b.x - focus.x, edge.b.y - focus.y, edge.b.z - focus.z)
                .rotate_y(az).rotate_x(el);
            let avg_z = (a.z + b.z) / 2.0;
            RotatedEdge { a, b, avg_z, cell_min: edge.cell_min, cell_max: edge.cell_max, color: edge.color }
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
            let depth = (edge.avg_z - z_min) / z_range;
            let brightness = 1.0 - depth * 0.75;
            let faded = fade_color(edge.color, brightness);

            // Check if any muon is inside this edge's parent cell bounding box
            let mut flash = 0.0_f64;
            for muon in &muon_hits {
                if muon.x >= edge.cell_min.x && muon.x <= edge.cell_max.x
                    && muon.y >= edge.cell_min.y && muon.y <= edge.cell_max.y
                    && muon.z >= edge.cell_min.z && muon.z <= edge.cell_max.z
                {
                    flash = 1.0;
                    break;
                }
            }

            let final_color = if flash > 0.1 {
                lerp_color(faded, Color::Rgb(255, 255, 255), 0.8)
            } else {
                faded
            };

            canvas.line_colored(sx0, sy0, sx1, sy1, final_color);
        }
    }

    // Animate muon particles if enabled
    if spec.particles {
        draw_particles(&mut canvas, az, el, camera_dist, fov, cx, cy, scale, &focus);
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
    focus: &Vec3,
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
            let n0 = to_norm(t0);
            let n1 = to_norm(t1);
            let p0 = Vec3::new(n0.x - focus.x, n0.y - focus.y, n0.z - focus.z)
                .rotate_y(az).rotate_x(el);
            let p1 = Vec3::new(n1.x - focus.x, n1.y - focus.y, n1.z - focus.z)
                .rotate_y(az).rotate_x(el);

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

/// Compute the 3D position of each active muon's head in normalized model coords.
/// Used to flash individual detector cells as muons pass through them.
fn compute_muon_positions() -> Vec<Vec3> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    // Same geometry constants as draw_particles
    let n_em: usize = 6;
    let n_had: usize = 4;
    let em_layer_t = 2.0 + 0.3;
    let had_layer_t = 40.0 + 3.0;
    let total_z = (n_em as f64) * em_layer_t + (n_had as f64) * had_layer_t;
    let max_xy = 90.0_f64;
    let norm_scale = 1.0 / (total_z.max(max_xy) * 0.5);
    let z_offset = total_z / 2.0;
    let source_z_mm = -50.0_f64;
    let z_end_mm = total_z + 20.0;
    let path_len = z_end_mm - source_z_mm;
    let spawn_interval = 2.0;
    let travel_time = 6.0;

    let hash = |seed: u64| -> f64 {
        let x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (x >> 33) as f64 / (1u64 << 31) as f64
    };

    let tower_half_width = 45.0_f64;
    let had_mid_z = 99.8_f64;
    let theta_max = (tower_half_width / (50.0 + had_mid_z)).atan();

    let max_slots = ((travel_time / spawn_interval) as f64).ceil() as u64 + 2;
    let mut positions = Vec::new();

    for i in 0..max_slots {
        let slot_time = (now / spawn_interval).floor() as u64 - i;
        let birth = slot_time as f64 * spawn_interval;
        let age = now - birth;

        if age < 0.0 || age > travel_time {
            continue;
        }

        let progress = age / travel_time;
        let h1 = hash(slot_time * 3);
        let h2 = hash(slot_time * 3 + 1);
        let cos_min = theta_max.cos();
        let cos_theta = 1.0 - h1 * (1.0 - cos_min);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let phi = h2 * 2.0 * std::f64::consts::PI;
        let dx = sin_theta * phi.cos();
        let dy = sin_theta * phi.sin();
        let dz = cos_theta;

        let head_z_mm = source_z_mm + progress * path_len;
        let t_head = (head_z_mm - source_z_mm) / dz;
        let x_mm = dx * t_head;
        let y_mm = dy * t_head;
        let z_mm = source_z_mm + dz * t_head;

        // Only flash while inside the detector (z_mm between 0 and total_z)
        if z_mm >= 0.0 && z_mm <= total_z {
            positions.push(Vec3::new(
                x_mm * norm_scale,
                y_mm * norm_scale,
                (z_mm - z_offset) * norm_scale,
            ));
        }
    }

    positions
}

/// Linearly interpolate between two RGB colors.
fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let t = t.clamp(0.0, 1.0);
            Color::Rgb(
                (r1 as f64 + (r2 as f64 - r1 as f64) * t) as u8,
                (g1 as f64 + (g2 as f64 - g1 as f64) * t) as u8,
                (b1 as f64 + (b2 as f64 - b1 as f64) * t) as u8,
            )
        }
        _ => a,
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
        let lines = render_wireframe(&spec, 40, 20, 0);
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
    fn test_parse_camera_keyframes() {
        let spec = parse_wireframe_spec(
            "model: detector\ncamera: dist=2.0 fz=-0.9 az=25 el=15\ncamera: dist=3.2 az=35 el=20",
        );
        assert_eq!(spec.cameras.len(), 2);
        assert!((spec.cameras[0].distance - 2.0).abs() < 0.01);
        assert!((spec.cameras[0].focus_z - -0.9).abs() < 0.01);
        assert!((spec.cameras[0].azimuth - 25.0).abs() < 0.01);
        assert!((spec.cameras[1].distance - 3.2).abs() < 0.01);
        assert!((spec.cameras[1].azimuth - 35.0).abs() < 0.01);
    }

    #[test]
    fn test_camera_lerp() {
        let a = CameraKeyframe { distance: 2.0, fov: 2.5, azimuth: 25.0, elevation: 15.0, focus_x: 0.0, focus_y: 0.0, focus_z: -0.9 };
        let b = CameraKeyframe { distance: 3.2, fov: 2.5, azimuth: 35.0, elevation: 20.0, focus_x: 0.0, focus_y: 0.0, focus_z: 0.0 };
        let mid = a.lerp(&b, 0.5);
        assert!((mid.distance - 2.6).abs() < 0.01);
        assert!((mid.azimuth - 30.0).abs() < 0.01);
        assert!((mid.focus_z - -0.45).abs() < 0.01);
    }

    #[test]
    fn test_behind_camera() {
        // Point behind camera should return None
        let p = project(Vec3::new(0.0, 0.0, -5.0), 4.0, 2.5);
        assert!(p.is_none());
    }
}
