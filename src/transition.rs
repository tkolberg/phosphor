use std::time::{Duration, Instant};

use rand::Rng;

/// Scramble characters ordered light → heavy (fewer pixels → more pixels).
/// The animation samples from the light end early and shifts toward heavy as it settles.
const SCRAMBLE_LIGHT: &[char] = &[
    '.', '·', ':', '\'', '`', ',', '-', '~',
    '⠁', '⠂', '⠄', '⠈', '⠐', '⠠',
    '╌', '╍', '┄', '┅',
    '░',
];

const SCRAMBLE_MID: &[char] = &[
    '╎', '╏', '/', '\\', '|', '+', '*',
    '⠋', '⠙', '⠹', '⠛', '⡇',
    '▘', '▝', '▖', '▗', '▚', '▞',
    '▒', '◌', '◍',
    'x', '>', '<', '=',
];

const SCRAMBLE_HEAVY: &[char] = &[
    '#', '@', '&', '%', 'X', 'A', 'Z', '0', '9',
    '⠿', '⣤', '⣶', '⣿',
    '▓', '█', '◎', '◉',
];

/// Total duration of the scramble animation.
const TRANSITION_DURATION: Duration = Duration::from_millis(400);

/// How long each character cycles before settling (max stagger).
const SETTLE_STAGGER: Duration = Duration::from_millis(300);

/// Minimum settle delay for the first character.
const SETTLE_BASE: Duration = Duration::from_millis(40);

/// How often the scramble characters change.
const CYCLE_INTERVAL: Duration = Duration::from_millis(30);

/// Direction the settle wave travels.
#[derive(Clone, Copy)]
pub enum TransitionDirection {
    /// Left-to-right, top-to-bottom (default for text slides).
    Forward,
    /// Bottom-to-top, left-to-right (natural for charts/graphs).
    BottomUp,
}

/// A snapshot of one cell in the terminal buffer.
#[derive(Clone)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<ratatui::style::Color>,
    pub bg: Option<ratatui::style::Color>,
    pub modifier: ratatui::style::Modifier,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            modifier: ratatui::style::Modifier::empty(),
        }
    }
}

/// The active transition state.
pub struct Transition {
    /// The target frame content (what we're revealing).
    target: Vec<Vec<Cell>>,
    /// Per-character settle time (when this char stops cycling).
    settle_at: Vec<Vec<Duration>>,
    /// When the transition started.
    started: Instant,
    /// Last time we changed the cycling characters.
    last_cycle: Instant,
    /// Current random characters for unsettled positions.
    random_chars: Vec<Vec<char>>,
    /// Width and height.
    width: usize,
    height: usize,
}

impl Transition {
    pub fn new(
        target: Vec<Vec<Cell>>,
        width: usize,
        height: usize,
        direction: TransitionDirection,
        prev_frame: Option<Vec<Vec<Cell>>>,
    ) -> Self {
        let now = Instant::now();
        let mut rng = rand::rng();

        // Determine which cells are new (need animation) vs unchanged (settle instantly)
        let is_new = |y: usize, x: usize| -> bool {
            if let Some(ref prev) = prev_frame {
                if y < prev.len() && x < prev[y].len() {
                    let old = &prev[y][x];
                    let new = &target[y][x];
                    // Cell changed if character or colors differ
                    return old.ch != new.ch || old.fg != new.fg || old.bg != new.bg;
                }
            }
            true // no previous frame means everything is new
        };

        let total_chars: usize = target
            .iter()
            .enumerate()
            .flat_map(|(y, row)| row.iter().enumerate().map(move |(x, c)| (y, x, c)))
            .filter(|(y, x, c)| c.ch != ' ' && is_new(*y, *x))
            .count();

        let mut settle_at = vec![vec![Duration::ZERO; width]; height];
        let mut char_index: usize = 0;

        match direction {
            TransitionDirection::Forward => {
                // Left-to-right, top-to-bottom
                for y in 0..height {
                    for x in 0..width {
                        if y < target.len()
                            && x < target[y].len()
                            && target[y][x].ch != ' '
                            && is_new(y, x)
                        {
                            let progress = if total_chars > 1 {
                                char_index as f64 / (total_chars - 1) as f64
                            } else {
                                0.0
                            };
                            settle_at[y][x] = SETTLE_BASE + SETTLE_STAGGER.mul_f64(progress);
                            char_index += 1;
                        }
                    }
                }
            }
            TransitionDirection::BottomUp => {
                // Bottom-to-top, left-to-right
                for y in (0..height).rev() {
                    for x in 0..width {
                        if y < target.len()
                            && x < target[y].len()
                            && target[y][x].ch != ' '
                            && is_new(y, x)
                        {
                            let progress = if total_chars > 1 {
                                char_index as f64 / (total_chars - 1) as f64
                            } else {
                                0.0
                            };
                            settle_at[y][x] = SETTLE_BASE + SETTLE_STAGGER.mul_f64(progress);
                            char_index += 1;
                        }
                    }
                }
            }
        }

        let random_chars: Vec<Vec<char>> = (0..height)
            .map(|_| {
                (0..width)
                    .map(|_| SCRAMBLE_LIGHT[rng.random_range(0..SCRAMBLE_LIGHT.len())])
                    .collect()
            })
            .collect();

        Transition {
            target,
            settle_at,
            started: now,
            last_cycle: now,
            random_chars,
            width,
            height,
        }
    }

    /// Is the transition complete?
    pub fn is_done(&self) -> bool {
        self.started.elapsed() >= TRANSITION_DURATION
    }

    /// Advance the animation — re-randomize cycling characters if enough time passed.
    /// Characters near the start of their cycle draw from light glyphs; as they
    /// approach their settle time, they shift toward heavier glyphs.
    pub fn tick(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_cycle) >= CYCLE_INTERVAL {
            let mut rng = rand::rng();
            let elapsed = self.started.elapsed();

            for y in 0..self.height {
                for x in 0..self.width {
                    let settle = self.settle_at[y][x];
                    if settle > Duration::ZERO && elapsed < settle {
                        // How far through this character's cycle are we? 0.0 = just started, 1.0 = about to settle
                        let progress = elapsed.as_secs_f64() / settle.as_secs_f64();
                        let pool = if progress < 0.4 {
                            SCRAMBLE_LIGHT
                        } else if progress < 0.75 {
                            SCRAMBLE_MID
                        } else {
                            SCRAMBLE_HEAVY
                        };
                        self.random_chars[y][x] = pool[rng.random_range(0..pool.len())];
                    }
                }
            }
            self.last_cycle = now;
        }
    }

    /// Get the character and style to display at (x, y) right now.
    pub fn get_cell(&self, x: usize, y: usize) -> Cell {
        if y >= self.height || x >= self.width {
            return Cell::default();
        }

        let elapsed = self.started.elapsed();
        let target = &self.target[y][x];

        if target.ch == ' ' || elapsed >= self.settle_at[y][x] {
            target.clone()
        } else {
            Cell {
                ch: self.random_chars[y][x],
                fg: target.fg,
                bg: target.bg,
                modifier: target.modifier,
            }
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }
}
