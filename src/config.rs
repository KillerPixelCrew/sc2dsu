use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// The DSU protocol exposes four controller ports.  Keep this independent of
/// the HID module so config stays usable by the headless server and tests.
pub const MAX_DSU_SLOTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axis {
    pub source: u8,
    pub invert: bool,
}

impl Axis {
    pub const fn new(source: u8, invert: bool) -> Self {
        Self { source, invert }
    }
    pub fn apply(&self, raw: [f32; 3]) -> f32 {
        let v = raw[(self.source as usize).min(2)];
        if self.invert { -v } else { v }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisMap {
    pub x: Axis,
    pub y: Axis,
    pub z: Axis,
}

impl AxisMap {
    // Gyro mapping matched to Eden/Yuzu Cemuhook UDP, which reads incoming DSU
    // as (pitch, roll, -yaw). See issue #3.
    pub const DEFAULT: Self = Self {
        x: Axis::new(0, false),
        y: Axis::new(2, true),
        z: Axis::new(1, false),
    };

    // Accel mapping for the same Eden/Yuzu remap: (accel.x, -accel.z, accel.y).
    pub const DEFAULT_ACCEL: Self = Self {
        x: Axis::new(0, true),
        y: Axis::new(2, true),
        z: Axis::new(1, false),
    };
}

impl Default for AxisMap {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// Bounds for the user-facing gyro sensitivity multiplier. Anything outside
// this range is almost certainly a config-file typo rather than a real intent.
pub const GYRO_SENSITIVITY_MIN: f32 = 0.10;
pub const GYRO_SENSITIVITY_MAX: f32 = 3.00;
pub const GYRO_SENSITIVITY_DEFAULT: f32 = 1.00;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub gyro: AxisMap,
    pub accel: AxisMap,
    pub gyro_sensitivity: f32,
    /// Optional, persistent controller-to-DSU-slot preferences.  Controller
    /// IDs are stable hashes of HID identity data, not user-visible names.
    /// Zero leaves a controller on the normal first-available-slot path.  TOML
    /// has no representation for `None` inside a homogeneous array.
    pub slot_assignments: [u64; MAX_DSU_SLOTS],
    pub start_minimized: bool,
    pub expose_to_network: bool,
    pub close_to_tray: bool,
    pub auto_calibrate: bool,
}

impl Config {
    pub const DEFAULT: Self = Self {
        port: 26760,
        gyro: AxisMap::DEFAULT,
        accel: AxisMap::DEFAULT_ACCEL,
        gyro_sensitivity: GYRO_SENSITIVITY_DEFAULT,
        slot_assignments: [0; MAX_DSU_SLOTS],
        start_minimized: false,
        expose_to_network: false,
        close_to_tray: false,
        auto_calibrate: true,
    };

    pub fn effective_gyro_sensitivity(&self) -> f32 {
        clamp_sensitivity(self.gyro_sensitivity)
    }

    pub fn slot_for_controller(&self, controller_id: u64) -> Option<u8> {
        self.slot_assignments
            .iter()
            .position(|assigned| *assigned == controller_id && controller_id != 0)
            .map(|slot| slot as u8)
    }

    /// Pins a controller to a DSU slot.  A controller may only occupy one
    /// preferred slot; moving it releases its old preference.  If another
    /// controller was pinned to the destination, it becomes automatic.
    pub fn assign_controller_to_slot(&mut self, controller_id: u64, slot: u8) {
        if usize::from(slot) >= MAX_DSU_SLOTS {
            return;
        }
        for assigned in &mut self.slot_assignments {
            if *assigned == controller_id {
                *assigned = 0;
            }
        }
        self.slot_assignments[usize::from(slot)] = controller_id;
    }

    pub fn clear_controller_assignment(&mut self, controller_id: u64) {
        for assigned in &mut self.slot_assignments {
            if *assigned == controller_id {
                *assigned = 0;
            }
        }
    }
}

pub fn clamp_sensitivity(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(GYRO_SENSITIVITY_MIN, GYRO_SENSITIVITY_MAX)
    } else {
        GYRO_SENSITIVITY_DEFAULT
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}

pub fn config_path() -> PathBuf {
    let dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.join("sc2dsu").join("config.toml")
}

pub fn load_or_create() -> Config {
    let path = config_path();
    if let Ok(s) = std::fs::read_to_string(&path) {
        match toml::from_str::<Config>(&s) {
            Ok(c) => {
                eprintln!("config: loaded {}", path.display());
                return c;
            }
            Err(e) => {
                eprintln!(
                    "config: {} is malformed ({e}); using defaults and not overwriting",
                    path.display()
                );
                return Config::default();
            }
        }
    }
    let c = Config::default();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = toml::to_string_pretty(&c) {
        match std::fs::write(&path, s) {
            Ok(()) => eprintln!("config: wrote default {}", path.display()),
            Err(e) => eprintln!("config: failed to write {}: {e}", path.display()),
        }
    }
    c
}

static LIVE: RwLock<Config> = RwLock::new(Config::DEFAULT);

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

pub fn bind_host(expose_to_network: bool) -> &'static str {
    if expose_to_network {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    }
}

pub fn install(initial: Config) {
    *LIVE.write().unwrap_or_else(|e| e.into_inner()) = initial;
    GENERATION.fetch_add(1, Ordering::Release);
}

pub fn snapshot() -> Config {
    LIVE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn update_and_save(new_cfg: Config) -> std::io::Result<()> {
    *LIVE.write().unwrap_or_else(|e| e.into_inner()) = new_cfg.clone();
    GENERATION.fetch_add(1, Ordering::Release);
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(&new_cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_apply_selects_and_inverts() {
        let raw = [1.0, 2.0, 3.0];
        assert_eq!(Axis::new(0, false).apply(raw), 1.0);
        assert_eq!(Axis::new(2, false).apply(raw), 3.0);
        assert_eq!(Axis::new(1, true).apply(raw), -2.0);
    }

    #[test]
    fn axis_apply_clamps_out_of_range_source() {
        let raw = [1.0, 2.0, 3.0];
        assert_eq!(Axis::new(7, false).apply(raw), 3.0);
    }

    #[test]
    fn config_toml_round_trips() {
        let original = Config::default();
        let text = toml::to_string_pretty(&original).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn config_partial_toml_uses_defaults() {
        let parsed: Config = toml::from_str("port = 12345").unwrap();
        assert_eq!(parsed.port, 12345);
        assert_eq!(parsed.gyro, AxisMap::DEFAULT);
        assert_eq!(parsed.accel, AxisMap::DEFAULT_ACCEL);
        assert!(!parsed.expose_to_network);
        assert!(parsed.auto_calibrate);
        assert_eq!(parsed.gyro_sensitivity, GYRO_SENSITIVITY_DEFAULT);
        assert_eq!(parsed.slot_assignments, [0; MAX_DSU_SLOTS]);
    }

    #[test]
    fn bind_host_maps_flag() {
        assert_eq!(bind_host(true), "0.0.0.0");
        assert_eq!(bind_host(false), "127.0.0.1");
    }

    #[test]
    fn clamp_sensitivity_handles_bounds_and_nan() {
        assert_eq!(clamp_sensitivity(1.0), 1.0);
        assert_eq!(clamp_sensitivity(0.0), GYRO_SENSITIVITY_MIN);
        assert_eq!(clamp_sensitivity(99.0), GYRO_SENSITIVITY_MAX);
        assert_eq!(clamp_sensitivity(-1.0), GYRO_SENSITIVITY_MIN);
        assert_eq!(clamp_sensitivity(f32::NAN), GYRO_SENSITIVITY_DEFAULT);
        assert_eq!(clamp_sensitivity(f32::INFINITY), GYRO_SENSITIVITY_DEFAULT);
    }

    #[test]
    fn controller_slot_assignment_moves_without_duplicates() {
        let mut cfg = Config::default();
        cfg.assign_controller_to_slot(10, 0);
        cfg.assign_controller_to_slot(20, 1);
        cfg.assign_controller_to_slot(10, 1);
        assert_eq!(cfg.slot_assignments, [0, 10, 0, 0]);
        assert_eq!(cfg.slot_for_controller(10), Some(1));
        assert_eq!(cfg.slot_for_controller(20), None);
        cfg.clear_controller_assignment(10);
        assert_eq!(cfg.slot_for_controller(10), None);
    }
}
