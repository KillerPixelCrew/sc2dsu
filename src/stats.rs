// ServerStats is split into sections by owning subsystem. Each writer can
// only mutate its own section, which structurally prevents one writer from
// clobbering fields it doesn't own (the bug the old flat `publish()` had).

use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

pub static RECENTER_REQUEST: AtomicBool = AtomicBool::new(false);
/// Zero means no targeted recenter request; otherwise this is a one-based DSU
/// slot number.
pub static RECENTER_SLOT_REQUEST: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub static RECALIBRATE_REQUEST: AtomicBool = AtomicBool::new(false);
/// Zero means no targeted calibration request; otherwise this is a one-based
/// DSU slot number.  One-based keeps the idle value unambiguous.
pub static RECALIBRATE_SLOT_REQUEST: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(0);

pub const MAX_SLOTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerInfo {
    pub id: u64,
    pub name: &'static str,
    pub transport: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ServerSection {
    pub subscribers: usize,
    pub controllers: usize,
    pub requests_per_sec: f32,
    pub samples_per_sec: f32,
    pub packets_per_sec: f32,
    pub device_active: bool,
    pub server_id: u32,
    pub bound_port: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct MotionSection {
    pub last_gyro_dps: [f32; 3],
    pub last_accel_g: [f32; 3],
    pub orientation: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct SlotSection {
    pub connected: bool,
    pub controller: Option<ControllerInfo>,
    pub motion: MotionSection,
    pub samples_per_sec: f32,
}

impl Default for SlotSection {
    fn default() -> Self {
        Self {
            connected: false,
            controller: None,
            motion: MotionSection::default(),
            samples_per_sec: 0.0,
        }
    }
}

impl Default for MotionSection {
    fn default() -> Self {
        Self {
            last_gyro_dps: [0.0; 3],
            last_accel_g: [0.0; 3],
            orientation: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CalibrationSection {
    pub active: bool,
    pub steady: bool,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ServerStats {
    pub server: ServerSection,
    pub motion: MotionSection,
    pub calibration: CalibrationSection,
    pub slots: [SlotSection; MAX_SLOTS],
}

static LIVE: RwLock<ServerStats> = RwLock::new(ServerStats {
    server: ServerSection {
        subscribers: 0,
        controllers: 0,
        requests_per_sec: 0.0,
        samples_per_sec: 0.0,
        packets_per_sec: 0.0,
        device_active: false,
        server_id: 0,
        bound_port: 0,
    },
    motion: MotionSection {
        last_gyro_dps: [0.0; 3],
        last_accel_g: [0.0; 3],
        orientation: [1.0, 0.0, 0.0, 0.0],
    },
    calibration: CalibrationSection {
        active: false,
        steady: false,
        confidence: 0.0,
    },
    slots: [SlotSection {
        connected: false,
        controller: None,
        motion: MotionSection {
            last_gyro_dps: [0.0; 3],
            last_accel_g: [0.0; 3],
            orientation: [1.0, 0.0, 0.0, 0.0],
        },
        samples_per_sec: 0.0,
    }; MAX_SLOTS],
});

pub fn snapshot() -> ServerStats {
    *LIVE.read().unwrap_or_else(|e| e.into_inner())
}

pub fn publish_server(s: ServerSection) {
    LIVE.write().unwrap_or_else(|e| e.into_inner()).server = s;
}

pub fn publish_motion(m: MotionSection) {
    LIVE.write().unwrap_or_else(|e| e.into_inner()).motion = m;
}

pub fn publish_calibration(c: CalibrationSection) {
    LIVE.write().unwrap_or_else(|e| e.into_inner()).calibration = c;
}

pub fn publish_slot_connected(slot: u8, controller: ControllerInfo) {
    if let Some(stats) = LIVE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .slots
        .get_mut(usize::from(slot))
    {
        stats.connected = true;
        stats.controller = Some(controller);
    }
}

pub fn publish_slot_disconnected(slot: u8) {
    if let Some(stats) = LIVE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .slots
        .get_mut(usize::from(slot))
    {
        *stats = SlotSection::default();
    }
}

pub fn publish_slot_motion(slot: u8, motion: MotionSection) {
    if let Some(stats) = LIVE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .slots
        .get_mut(usize::from(slot))
    {
        stats.motion = motion;
    }
}

pub fn publish_slot_rate(slot: u8, samples_per_sec: f32) {
    if let Some(stats) = LIVE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .slots
        .get_mut(usize::from(slot))
    {
        stats.samples_per_sec = samples_per_sec;
    }
}
