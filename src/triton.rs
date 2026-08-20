use crate::config;
use crate::gyro_calibration::GyroCalibration;
use crate::stats::{self, ControllerInfo};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use std::time::{Duration, Instant};

pub const VID_VALVE: u16 = 0x28DE;
pub const MAX_CONTROLLERS: usize = 4;
pub const PID_STEAM_CHELL: u16 = 0x1101;
pub const PID_STEAM_WIRED: u16 = 0x1102;
pub const PID_STEAM_BLE: u16 = 0x1105;
pub const PID_STEAM_BLE2: u16 = 0x1106;
pub const PID_STEAM_DONGLE: u16 = 0x1142;
pub const PID_TRITON_WIRED: u16 = 0x1302;
pub const PID_TRITON_BLE: u16 = 0x1303;
pub const PID_PROTEUS_DONGLE: u16 = 0x1304;
pub const PID_NEREID_DONGLE: u16 = 0x1305;

const FEATURE_REPORT_ID: u8 = 0x01;
const FEATURE_REPORT_BYTES: usize = 64;
const INPUT_REPORT_BYTES: usize = 64;
const ID_SET_SETTINGS_VALUES: u8 = 0x87;
const SETTING_LIZARD_MODE: u8 = 9;
const SETTING_IMU_MODE: u8 = 48;
const LIZARD_MODE_OFF: u16 = 0;
const GYRO_MODE_RAW_ACCEL_AND_GYRO: u16 = 0x0008 | 0x0010;

// Original (2015/D0G) Steam Controller protocol. These values and the BLE
// segmentation below mirror SDL3's SDL_hidapi_steam.c and steam headers.
const STEAM_FEATURE_REPORT_BYTES: usize = 65;
const STEAM_BLE_REPORT_ID: u8 = 0x03;
const STEAM_BLE_SEGMENT_BYTES: usize = 20;
const STEAM_BLE_SEGMENT_PAYLOAD_BYTES: usize = 18;
const STEAM_BLE_SEGMENT_DATA: u8 = 0x80;
const STEAM_BLE_SEGMENT_LAST: u8 = 0x40;
const STEAM_BLE_STATE: u8 = 0x04;

const ID_CLEAR_DIGITAL_MAPPINGS: u8 = 0x81;
const ID_GET_ATTRIBUTES_VALUES: u8 = 0x83;
const ID_SET_DEFAULT_DIGITAL_MAPPINGS: u8 = 0x85;
const ID_LOAD_DEFAULT_SETTINGS: u8 = 0x8E;
const SETTING_LEFT_TRACKPAD_MODE: u8 = 7;
const SETTING_RIGHT_TRACKPAD_MODE: u8 = 8;
const SETTING_SMOOTH_ABSOLUTE_MOUSE: u8 = 24;
const SETTING_WIRELESS_PACKET_VERSION: u8 = 49;
const TRACKPAD_ABSOLUTE_MOUSE: u16 = 0;
const TRACKPAD_NONE: u16 = 7;

const STEAM_BLE_BUTTON_CHUNK_1: u16 = 0x0010;
const STEAM_BLE_BUTTON_CHUNK_2: u16 = 0x0020;
const STEAM_BLE_BUTTON_CHUNK_3: u16 = 0x0040;
const STEAM_BLE_LEFT_JOYSTICK_CHUNK: u16 = 0x0080;
const STEAM_BLE_LEFT_TRACKPAD_CHUNK: u16 = 0x0100;
const STEAM_BLE_RIGHT_TRACKPAD_CHUNK: u16 = 0x0200;
const STEAM_BLE_IMU_ACCEL_CHUNK: u16 = 0x0400;
const STEAM_BLE_IMU_GYRO_CHUNK: u16 = 0x0800;
const STEAM_BLE_IMU_QUAT_CHUNK: u16 = 0x1000;

const STEAM_RIGHT_BUMPER: u64 = 0x0000_0004;
const STEAM_LEFT_BUMPER: u64 = 0x0000_0008;
const STEAM_Y: u64 = 0x0000_0010;
const STEAM_B: u64 = 0x0000_0020;
const STEAM_X: u64 = 0x0000_0040;
const STEAM_A: u64 = 0x0000_0080;
const STEAM_DPAD_UP: u64 = 0x0000_0100;
const STEAM_DPAD_RIGHT: u64 = 0x0000_0200;
const STEAM_DPAD_LEFT: u64 = 0x0000_0400;
const STEAM_DPAD_DOWN: u64 = 0x0000_0800;
const STEAM_BACK: u64 = 0x0000_1000;
const STEAM_GUIDE: u64 = 0x0000_2000;
const STEAM_START: u64 = 0x0000_4000;
const STEAM_LEFT_PADDLE: u64 = 0x0000_8000;
const STEAM_RIGHT_PADDLE: u64 = 0x0001_0000;
const STEAM_LEFT_PAD_CLICKED: u64 = 0x0002_0000;
const STEAM_RIGHT_PAD_CLICKED: u64 = 0x0004_0000;
const STEAM_LEFT_PAD_TOUCHED: u64 = 0x0008_0000;
const STEAM_RIGHT_PAD_TOUCHED: u64 = 0x0010_0000;
const STEAM_LEFT_PAD_AND_JOYSTICK: u64 = 0x0080_0000;
const STEAM_JOYSTICK_CLICKED: u64 = 0x0040_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Triton,
    Steam2015 { bluetooth: bool },
}

pub const TRITON_REPORT_STATE: u8 = 0x42;
pub const TRITON_REPORT_STATE_BLE: u8 = 0x45;
#[allow(dead_code)]
pub const TRITON_REPORT_BATTERY: u8 = 0x43;
#[allow(dead_code)]
pub const TRITON_REPORT_WIRELESS_X: u8 = 0x46;
#[allow(dead_code)]
pub const TRITON_REPORT_WIRELESS: u8 = 0x79;

const LIZARD_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const IMU_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy)]
pub struct ImuSample {
    pub timestamp_us: u32,
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
}

#[allow(dead_code)]
pub mod button {
    pub const A: u32 = 0x0000_0001;
    pub const B: u32 = 0x0000_0002;
    pub const X: u32 = 0x0000_0004;
    pub const Y: u32 = 0x0000_0008;
    pub const QAM: u32 = 0x0000_0010;
    pub const R3: u32 = 0x0000_0020;
    pub const VIEW: u32 = 0x0000_0040;
    pub const R4: u32 = 0x0000_0080;
    pub const R5: u32 = 0x0000_0100;
    pub const R: u32 = 0x0000_0200;
    pub const DPAD_DOWN: u32 = 0x0000_0400;
    pub const DPAD_RIGHT: u32 = 0x0000_0800;
    pub const DPAD_LEFT: u32 = 0x0000_1000;
    pub const DPAD_UP: u32 = 0x0000_2000;
    pub const MENU: u32 = 0x0000_4000;
    pub const L3: u32 = 0x0000_8000;
    pub const STEAM: u32 = 0x0001_0000;
    pub const L4: u32 = 0x0002_0000;
    pub const L5: u32 = 0x0004_0000;
    pub const L: u32 = 0x0008_0000;
}

#[derive(Debug, Clone, Copy)]
pub struct ControllerState {
    pub buttons: u32,
    pub trigger_left: u16,
    pub trigger_right: u16,
    pub left_stick: [i16; 2],
    pub right_stick: [i16; 2],
    pub imu: ImuSample,
}

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Connected {
        slot: u8,
        controller: ControllerInfo,
    },
    Sample {
        slot: u8,
        state: ControllerState,
    },
    Disconnected {
        slot: u8,
    },
}

pub fn pid_label(pid: u16) -> &'static str {
    match pid {
        PID_STEAM_CHELL => "Steam Controller (legacy)",
        PID_STEAM_WIRED => "Steam Controller 2015 wired",
        PID_STEAM_BLE | PID_STEAM_BLE2 => "Steam Controller 2015 Bluetooth",
        PID_STEAM_DONGLE => "Steam Controller 2015 dongle",
        PID_TRITON_WIRED => "Triton wired",
        PID_TRITON_BLE => "Triton BLE",
        PID_PROTEUS_DONGLE => "Proteus Puck",
        PID_NEREID_DONGLE => "Nereid dongle",
        _ => "?",
    }
}

fn controller_info(info: &DeviceInfo) -> ControllerInfo {
    let product_id = info.product_id();
    let transport = match product_id {
        PID_STEAM_BLE | PID_STEAM_BLE2 | PID_TRITON_BLE => "Bluetooth",
        PID_STEAM_DONGLE | PID_PROTEUS_DONGLE | PID_NEREID_DONGLE => "Wireless",
        _ => "USB",
    };
    ControllerInfo {
        id: stable_controller_id(info),
        name: if is_steam_2015_pid(product_id) {
            "Steam Controller (2015)"
        } else {
            "Steam Controller (2026)"
        },
        transport,
    }
}

/// HID paths are stable for an attached interface, while serials (when a
/// transport exposes one) survive reconnection.  Including the interface
/// number distinguishes the four receiver ports of a Puck.
fn stable_controller_id(info: &DeviceInfo) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut id = OFFSET;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            id ^= u64::from(*byte);
            id = id.wrapping_mul(PRIME);
        }
    };
    add(&info.vendor_id().to_le_bytes());
    add(&info.product_id().to_le_bytes());
    add(&info.interface_number().to_le_bytes());
    if let Some(serial) = info.serial_number() {
        add(serial.as_bytes());
    } else {
        add(info.path().to_bytes());
    }
    id
}

pub fn is_steam_2015_pid(pid: u16) -> bool {
    matches!(
        pid,
        PID_STEAM_CHELL | PID_STEAM_WIRED | PID_STEAM_BLE | PID_STEAM_BLE2 | PID_STEAM_DONGLE
    )
}

pub fn is_triton_pid(pid: u16) -> bool {
    matches!(
        pid,
        PID_TRITON_WIRED | PID_TRITON_BLE | PID_PROTEUS_DONGLE | PID_NEREID_DONGLE
    )
}

pub fn is_supported_pid(pid: u16) -> bool {
    is_steam_2015_pid(pid) || is_triton_pid(pid)
}

pub fn is_candidate(info: &DeviceInfo) -> bool {
    if info.vendor_id() != VID_VALVE
        || !is_supported_pid(info.product_id())
        || info.usage_page() < 0xFF00
    {
        return false;
    }
    match info.product_id() {
        PID_STEAM_BLE | PID_STEAM_BLE2 => true,
        PID_STEAM_DONGLE => (1..=4).contains(&info.interface_number()),
        PID_STEAM_CHELL | PID_STEAM_WIRED => info.interface_number() == 2,
        PID_PROTEUS_DONGLE | PID_NEREID_DONGLE => (2..=5).contains(&info.interface_number()),
        PID_TRITON_WIRED | PID_TRITON_BLE => true,
        _ => false,
    }
}

fn protocol_for_pid(pid: u16) -> Option<Protocol> {
    if is_triton_pid(pid) {
        Some(Protocol::Triton)
    } else if is_steam_2015_pid(pid) {
        Some(Protocol::Steam2015 {
            bluetooth: matches!(pid, PID_STEAM_BLE | PID_STEAM_BLE2),
        })
    } else {
        None
    }
}

pub fn list_candidates(api: &HidApi) -> Vec<DeviceInfo> {
    let mut candidates: Vec<_> = api
        .device_list()
        .filter(|d| is_candidate(d))
        .cloned()
        .collect();

    // Directly-connected controllers are preferable to dongles. In
    // particular, Windows keeps an idle Puck enumerated when no Triton is
    // connected; trying it first would otherwise starve an active BLE pad.
    candidates.sort_by_key(|d| match d.product_id() {
        PID_STEAM_BLE | PID_STEAM_BLE2 => 0,
        PID_STEAM_CHELL | PID_STEAM_WIRED => 1,
        PID_STEAM_DONGLE => 2,
        PID_TRITON_WIRED | PID_TRITON_BLE => 3,
        PID_PROTEUS_DONGLE | PID_NEREID_DONGLE => 4,
        _ => 5,
    });
    candidates
}

fn build_set_setting_report(setting_num: u8, setting_value: u16) -> [u8; FEATURE_REPORT_BYTES] {
    let mut buf = [0u8; FEATURE_REPORT_BYTES];
    buf[0] = FEATURE_REPORT_ID;
    buf[1] = ID_SET_SETTINGS_VALUES;
    buf[2] = 3;
    buf[3] = setting_num;
    let v = setting_value.to_le_bytes();
    buf[4] = v[0];
    buf[5] = v[1];
    buf
}

fn send_steam_feature(dev: &HidDevice, bluetooth: bool, message: &[u8]) -> Result<(), String> {
    if message.is_empty() || message.len() > STEAM_FEATURE_REPORT_BYTES - 1 {
        return Err("invalid Steam feature report length".into());
    }

    if bluetooth {
        for (segment_number, chunk) in message.chunks(STEAM_BLE_SEGMENT_PAYLOAD_BYTES).enumerate() {
            let mut segment = [0u8; STEAM_BLE_SEGMENT_BYTES];
            segment[0] = STEAM_BLE_REPORT_ID;
            segment[1] = STEAM_BLE_SEGMENT_DATA | segment_number as u8;
            if (segment_number + 1) * STEAM_BLE_SEGMENT_PAYLOAD_BYTES >= message.len() {
                segment[1] |= STEAM_BLE_SEGMENT_LAST;
            }
            segment[2..2 + chunk.len()].copy_from_slice(chunk);
            dev.send_feature_report(&segment)
                .map_err(|e| format!("Bluetooth feature 0x{:02X}: {e}", message[0]))?;
        }
    } else {
        let mut report = [0u8; STEAM_FEATURE_REPORT_BYTES];
        report[1..1 + message.len()].copy_from_slice(message);
        let mut last_error = None;
        for _ in 0..50 {
            match dev.send_feature_report(&report) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    // SDL's wireless dongle firmware reports "failure" while
                    // the radio round trip is still pending.
                    std::thread::sleep(Duration::from_micros(500));
                }
            }
        }
        return Err(format!(
            "feature 0x{:02X}: {}",
            message[0],
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "no response".into())
        ));
    }
    Ok(())
}

fn steam_settings_message(settings: &[(u8, u16)]) -> Vec<u8> {
    let mut message = Vec::with_capacity(2 + settings.len() * 3);
    message.push(ID_SET_SETTINGS_VALUES);
    message.push((settings.len() * 3) as u8);
    for &(number, value) in settings {
        message.push(number);
        message.extend_from_slice(&value.to_le_bytes());
    }
    message
}

fn set_steam_2015_input_mode(dev: &HidDevice, bluetooth: bool) -> Result<(), String> {
    send_steam_feature(
        dev,
        bluetooth,
        &steam_settings_message(&[
            (SETTING_WIRELESS_PACKET_VERSION, 2),
            (SETTING_LEFT_TRACKPAD_MODE, TRACKPAD_NONE),
            (SETTING_RIGHT_TRACKPAD_MODE, TRACKPAD_NONE),
            (SETTING_SMOOTH_ABSOLUTE_MOUSE, 0),
        ]),
    )
}

fn enable_steam_2015_imu(dev: &HidDevice, bluetooth: bool) -> Result<(), String> {
    send_steam_feature(
        dev,
        bluetooth,
        &steam_settings_message(&[(SETTING_IMU_MODE, GYRO_MODE_RAW_ACCEL_AND_GYRO)]),
    )
}

fn read_steam_feature(
    dev: &HidDevice,
    bluetooth: bool,
    expected_response: u8,
) -> Result<(), String> {
    if bluetooth {
        let mut assembler = SteamPacketAssembler::default();
        for _ in 0..16 {
            // SDL reads one extra byte on Windows because the OS exposes two
            // copies of BLE report ID 3. SteamPacketAssembler normalizes both
            // the 20-byte and duplicated 21-byte forms.
            let mut segment = [0u8; STEAM_BLE_SEGMENT_BYTES + 1];
            segment[0] = STEAM_BLE_REPORT_ID;
            match dev.get_feature_report(&mut segment) {
                Ok(n) if n > 0 => {
                    if let Some(packet_len) = assembler.push(&segment[..n])? {
                        if assembler.buffer[..packet_len].first().copied()
                            == Some(expected_response)
                        {
                            return Ok(());
                        }
                        assembler.reset();
                    }
                }
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_micros(500)),
            }
        }
    } else {
        for _ in 0..50 {
            let mut report = [0u8; STEAM_FEATURE_REPORT_BYTES];
            match dev.get_feature_report(&mut report) {
                Ok(n) if n > 1 && report[1] == expected_response => return Ok(()),
                _ => std::thread::sleep(Duration::from_micros(500)),
            }
        }
    }
    Err(format!(
        "no feature response 0x{expected_response:02X} from controller"
    ))
}

fn configure_steam_2015(dev: &HidDevice, bluetooth: bool) -> Result<(), String> {
    // This is SDL3's reset sequence: remove the firmware's keyboard/mouse
    // mappings, restore known settings, select compact BLE packets, and then
    // enable raw accelerometer + gyro samples.
    send_steam_feature(dev, bluetooth, &[ID_GET_ATTRIBUTES_VALUES])?;
    read_steam_feature(dev, bluetooth, ID_GET_ATTRIBUTES_VALUES)?;
    send_steam_feature(dev, bluetooth, &[ID_CLEAR_DIGITAL_MAPPINGS])?;
    send_steam_feature(dev, bluetooth, &[ID_LOAD_DEFAULT_SETTINGS, 0])?;
    set_steam_2015_input_mode(dev, bluetooth)?;
    enable_steam_2015_imu(dev, bluetooth)
}

fn restore_steam_2015(dev: &HidDevice, bluetooth: bool) {
    // Match SDL3's close path so the controller retains its normal desktop
    // ("lizard mode") behavior after the DSU client disconnects.
    let _ = send_steam_feature(dev, bluetooth, &[ID_SET_DEFAULT_DIGITAL_MAPPINGS]);
    let _ = send_steam_feature(dev, bluetooth, &[ID_LOAD_DEFAULT_SETTINGS, 0]);
    let _ = send_steam_feature(
        dev,
        bluetooth,
        &steam_settings_message(&[(SETTING_RIGHT_TRACKPAD_MODE, TRACKPAD_ABSOLUTE_MOUSE)]),
    );
}

#[derive(Debug)]
struct SteamPacketAssembler {
    buffer: [u8; STEAM_BLE_SEGMENT_PAYLOAD_BYTES * 8],
    expected_segment: u8,
}

impl Default for SteamPacketAssembler {
    fn default() -> Self {
        Self {
            buffer: [0; STEAM_BLE_SEGMENT_PAYLOAD_BYTES * 8],
            expected_segment: 0,
        }
    }
}

impl SteamPacketAssembler {
    fn reset(&mut self) {
        self.buffer.fill(0);
        self.expected_segment = 0;
    }

    fn push(&mut self, segment: &[u8]) -> Result<Option<usize>, String> {
        // Some HID backends expose a duplicated report ID. Accept that form
        // as well as hidapi's normal 20-byte input report.
        let segment = if segment.len() > STEAM_BLE_SEGMENT_BYTES
            && segment[0] == STEAM_BLE_REPORT_ID
            && segment[1] == STEAM_BLE_REPORT_ID
        {
            &segment[1..]
        } else {
            segment
        };
        if segment.first().copied() != Some(STEAM_BLE_REPORT_ID) {
            return Ok(None); // Keyboard/mouse report while lizard mode winds down.
        }
        if segment.len() < STEAM_BLE_SEGMENT_BYTES {
            self.reset();
            return Err(format!("short Bluetooth segment ({} bytes)", segment.len()));
        }

        let header = segment[1];
        if header & STEAM_BLE_SEGMENT_DATA == 0 {
            return Ok(None);
        }
        let number = header & 0x07;
        if number != self.expected_segment {
            self.reset();
            if number != 0 {
                return Ok(None);
            }
        }

        let offset = number as usize * STEAM_BLE_SEGMENT_PAYLOAD_BYTES;
        self.buffer[offset..offset + STEAM_BLE_SEGMENT_PAYLOAD_BYTES]
            .copy_from_slice(&segment[2..STEAM_BLE_SEGMENT_BYTES]);
        if header & STEAM_BLE_SEGMENT_LAST != 0 {
            self.expected_segment = 0;
            Ok(Some(offset + STEAM_BLE_SEGMENT_PAYLOAD_BYTES))
        } else {
            self.expected_segment = number + 1;
            Ok(None)
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SteamRawState {
    buttons: u64,
    trigger_left: u16,
    trigger_right: u16,
    left_stick: [i16; 2],
    left_pad: [i16; 2],
    right_pad: [i16; 2],
    accel: [i16; 3],
    gyro: [i16; 3],
}

fn steam_trigger(value: u8) -> u16 {
    const STEAM_TRIGGER_MAX: u32 = 26_000;
    let expanded = u32::from(value) * 129;
    ((expanded * i16::MAX as u32) / STEAM_TRIGGER_MAX).min(i16::MAX as u32) as u16
}

fn take_bytes<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Option<&'a [u8]> {
    let value = data.get(*offset..offset.checked_add(len)?)?;
    *offset += len;
    Some(value)
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn pair_at(data: &[u8], offset: usize) -> Option<[i16; 2]> {
    Some([i16_at(data, offset)?, i16_at(data, offset + 2)?])
}

fn triple_at(data: &[u8], offset: usize) -> Option<[i16; 3]> {
    Some([
        i16_at(data, offset)?,
        i16_at(data, offset + 2)?,
        i16_at(data, offset + 4)?,
    ])
}

fn update_steam_ble_state(data: &[u8], state: &mut SteamRawState) -> Option<()> {
    if data.len() < 2 || data[0] & 0x0F != STEAM_BLE_STATE {
        return None;
    }
    let chunks = u16::from(data[0] & 0xF0) | (u16::from(data[1]) << 8);
    let mut offset = 2;

    if chunks & STEAM_BLE_BUTTON_CHUNK_1 != 0 {
        let bytes = take_bytes(data, &mut offset, 3)?;
        state.buttons = (state.buttons & !0x00FF_FFFF)
            | u64::from(bytes[0])
            | (u64::from(bytes[1]) << 8)
            | (u64::from(bytes[2]) << 16);
    }
    if chunks & STEAM_BLE_BUTTON_CHUNK_2 != 0 {
        let bytes = take_bytes(data, &mut offset, 2)?;
        state.trigger_left = steam_trigger(bytes[0]);
        state.trigger_right = steam_trigger(bytes[1]);
    }
    if chunks & STEAM_BLE_BUTTON_CHUNK_3 != 0 {
        let bytes = take_bytes(data, &mut offset, 3)?;
        for (index, value) in bytes.iter().enumerate() {
            let shift = 40 + index * 8;
            state.buttons = (state.buttons & !(0xFFu64 << shift)) | (u64::from(*value) << shift);
        }
    }
    if chunks & STEAM_BLE_LEFT_JOYSTICK_CHUNK != 0 {
        let bytes = take_bytes(data, &mut offset, 4)?;
        state.left_stick = pair_at(bytes, 0)?;
    }
    if chunks & STEAM_BLE_LEFT_TRACKPAD_CHUNK != 0 {
        let bytes = take_bytes(data, &mut offset, 4)?;
        state.left_pad = pair_at(bytes, 0)?;
    }
    if chunks & STEAM_BLE_RIGHT_TRACKPAD_CHUNK != 0 {
        let bytes = take_bytes(data, &mut offset, 4)?;
        state.right_pad = pair_at(bytes, 0)?;
    }
    if chunks & STEAM_BLE_IMU_ACCEL_CHUNK != 0 {
        let bytes = take_bytes(data, &mut offset, 6)?;
        state.accel = triple_at(bytes, 0)?;
    }
    if chunks & STEAM_BLE_IMU_GYRO_CHUNK != 0 {
        let bytes = take_bytes(data, &mut offset, 6)?;
        state.gyro = triple_at(bytes, 0)?;
    }
    if chunks & STEAM_BLE_IMU_QUAT_CHUNK != 0 {
        let _ = take_bytes(data, &mut offset, 8)?;
    }
    Some(())
}

fn update_steam_full_state(mut data: &[u8], state: &mut SteamRawState) -> Option<()> {
    // A zero report ID may be retained by some HID backends.
    if data.len() >= 4 && data[0] == 0 && data[1] == 1 && data[2] == 0 {
        data = &data[1..];
    }
    if data.len() < 24 || data[0..2] != [1, 0] {
        return None;
    }
    let report_type = data[2];
    if !matches!(report_type, 1 | 7) {
        return None;
    }

    let button_word = u64::from_le_bytes(data.get(8..16)?.try_into().ok()?);
    state.buttons = button_word & !(0xFFFFu64 << 24);
    state.trigger_left = steam_trigger(data[11]);
    state.trigger_right = steam_trigger(data[12]);

    let packed_left = pair_at(data, 16)?;
    if state.buttons & STEAM_LEFT_PAD_TOUCHED != 0 {
        state.left_pad = packed_left;
        if state.buttons & STEAM_LEFT_PAD_AND_JOYSTICK == 0 {
            state.left_stick = [0, 0];
        }
    } else {
        state.left_stick = packed_left;
        if state.buttons & STEAM_LEFT_PAD_AND_JOYSTICK == 0 {
            state.left_pad = [0, 0];
        }
    }
    state.right_pad = pair_at(data, 20)?;

    match report_type {
        1 => {
            if data.len() < 40 {
                return None;
            }
            state.accel = triple_at(data, 28)?;
            state.gyro = triple_at(data, 34)?;
        }
        7 => {
            if data.len() < 31 {
                return None;
            }
            match data[24] {
                2 => state.accel = triple_at(data, 25)?,
                3 => state.gyro = triple_at(data, 25)?,
                _ => {}
            }
        }
        _ => unreachable!(),
    }
    Some(())
}

fn rotate_pad([x, y]: [i16; 2], angle: f32, touched: bool) -> [i16; 2] {
    let x = f32::from(x);
    let y = f32::from(y);
    let offset = if touched { 1000.0 } else { 0.0 };
    let rotated_x = angle.cos() * x - angle.sin() * y + offset;
    let rotated_y = angle.sin() * x + angle.cos() * y + offset;
    [
        rotated_x.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16,
        rotated_y.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16,
    ]
}

fn steam_buttons(bits: u64) -> u32 {
    let mut buttons = 0;
    let map = [
        (STEAM_A, button::A),
        (STEAM_B, button::B),
        (STEAM_X, button::X),
        (STEAM_Y, button::Y),
        (STEAM_RIGHT_BUMPER, button::R),
        (STEAM_LEFT_BUMPER, button::L),
        (STEAM_DPAD_UP, button::DPAD_UP),
        (STEAM_DPAD_RIGHT, button::DPAD_RIGHT),
        (STEAM_DPAD_LEFT, button::DPAD_LEFT),
        (STEAM_DPAD_DOWN, button::DPAD_DOWN),
        (STEAM_BACK, button::VIEW),
        (STEAM_START, button::MENU),
        (STEAM_GUIDE, button::STEAM),
        (STEAM_LEFT_PADDLE, button::L4),
        (STEAM_RIGHT_PADDLE, button::R4),
        (STEAM_RIGHT_PAD_CLICKED, button::R3),
    ];
    for (steam, internal) in map {
        if bits & steam != 0 {
            buttons |= internal;
        }
    }
    if bits & STEAM_JOYSTICK_CLICKED != 0
        || (bits & STEAM_LEFT_PAD_CLICKED != 0 && bits & STEAM_LEFT_PAD_TOUCHED == 0)
    {
        buttons |= button::L3;
    }
    buttons
}

fn steam_controller_state(
    raw: &SteamRawState,
    timestamp_us: u32,
    gyro_map: &config::AxisMap,
    accel_map: &config::AxisMap,
) -> ControllerState {
    let to_g = |v: i16| (f32::from(v) / 32768.0) * 2.0;
    let to_dps = |v: i16| (f32::from(v) / 32768.0) * 2000.0;
    let raw_accel = [to_g(raw.accel[0]), to_g(raw.accel[1]), to_g(raw.accel[2])];
    // SDL3 shows that the 2015 pad's raw gyro Y orientation is opposite to
    // Triton's. Normalize it before applying the shared user axis mapping.
    let raw_gyro = [
        to_dps(raw.gyro[0]),
        -to_dps(raw.gyro[1]),
        to_dps(raw.gyro[2]),
    ];
    let right_pad = rotate_pad(
        raw.right_pad,
        15.0_f32.to_radians(),
        raw.buttons & STEAM_RIGHT_PAD_TOUCHED != 0,
    );
    ControllerState {
        buttons: steam_buttons(raw.buttons),
        trigger_left: raw.trigger_left,
        trigger_right: raw.trigger_right,
        left_stick: [raw.left_stick[0], !raw.left_stick[1]],
        right_stick: [right_pad[0], !right_pad[1]],
        imu: ImuSample {
            timestamp_us,
            accel_g: [
                accel_map.x.apply(raw_accel),
                accel_map.y.apply(raw_accel),
                accel_map.z.apply(raw_accel),
            ],
            gyro_dps: [
                gyro_map.x.apply(raw_gyro),
                gyro_map.y.apply(raw_gyro),
                gyro_map.z.apply(raw_gyro),
            ],
        },
    }
}

pub fn parse_imu(
    payload: &[u8],
    gyro_map: &config::AxisMap,
    accel_map: &config::AxisMap,
) -> Option<ImuSample> {
    const IMU_OFFSET: usize = 29;
    const IMU_NOQUAT_LEN: usize = 16;
    if payload.len() < IMU_OFFSET + IMU_NOQUAT_LEN {
        return None;
    }
    let imu = &payload[IMU_OFFSET..];
    let ts = u32::from_le_bytes([imu[0], imu[1], imu[2], imu[3]]);
    let i16le = |o: usize| i16::from_le_bytes([imu[o], imu[o + 1]]);
    let raw_accel = (i16le(4), i16le(6), i16le(8));
    let raw_gyro = (i16le(10), i16le(12), i16le(14));

    let to_g = |v: i16| (v as f32 / 32768.0) * 2.0;
    let to_dps = |v: i16| (v as f32 / 32768.0) * 2000.0;
    let raw_accel_f = [to_g(raw_accel.0), to_g(raw_accel.1), to_g(raw_accel.2)];
    let raw_gyro_f = [to_dps(raw_gyro.0), to_dps(raw_gyro.1), to_dps(raw_gyro.2)];

    Some(ImuSample {
        timestamp_us: ts,
        accel_g: [
            accel_map.x.apply(raw_accel_f),
            accel_map.y.apply(raw_accel_f),
            accel_map.z.apply(raw_accel_f),
        ],
        gyro_dps: [
            gyro_map.x.apply(raw_gyro_f),
            gyro_map.y.apply(raw_gyro_f),
            gyro_map.z.apply(raw_gyro_f),
        ],
    })
}

pub fn parse_state(
    payload: &[u8],
    gyro_map: &config::AxisMap,
    accel_map: &config::AxisMap,
) -> Option<ControllerState> {
    let imu = parse_imu(payload, gyro_map, accel_map)?;
    let u16le = |o: usize| u16::from_le_bytes([payload[o], payload[o + 1]]);
    let i16le = |o: usize| i16::from_le_bytes([payload[o], payload[o + 1]]);
    Some(ControllerState {
        buttons: u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]),
        trigger_left: u16le(5),
        trigger_right: u16le(7),
        left_stick: [i16le(9), i16le(11)],
        right_stick: [i16le(13), i16le(15)],
        imu,
    })
}

pub struct OpenSlot {
    dev: HidDevice,
    protocol: Protocol,
    steam_assembler: SteamPacketAssembler,
    steam_state: SteamRawState,
    steam_epoch: Instant,
    last_lizard_refresh: Instant,
    last_imu_refresh: Instant,
    gyro_map: config::AxisMap,
    accel_map: config::AxisMap,
    gyro_sensitivity: f32,
    gyro_cal: GyroCalibration,
    auto_calibrate: bool,
    last_imu_ts_us: Option<u32>,
    cfg_generation: u64,
    pub interface_number: i32,
    pub product_id: u16,
    pub controller: ControllerInfo,
    pub input_reports_seen: u32,
}

impl OpenSlot {
    pub fn open(api: &HidApi, info: &DeviceInfo) -> Result<Self, String> {
        let protocol = protocol_for_pid(info.product_id())
            .ok_or_else(|| format!("unsupported PID {:04X}", info.product_id()))?;
        let dev = api
            .open_path(info.path())
            .map_err(|e| format!("open: {e}"))?;
        match protocol {
            Protocol::Triton => {
                let lizard = build_set_setting_report(SETTING_LIZARD_MODE, LIZARD_MODE_OFF);
                dev.send_feature_report(&lizard)
                    .map_err(|e| format!("lizard-off: {e}"))?;
                let imu = build_set_setting_report(SETTING_IMU_MODE, GYRO_MODE_RAW_ACCEL_AND_GYRO);
                dev.send_feature_report(&imu)
                    .map_err(|e| format!("imu-on: {e}"))?;
            }
            Protocol::Steam2015 { bluetooth } => configure_steam_2015(&dev, bluetooth)?,
        }
        let _ = dev.set_blocking_mode(false);
        let cfg_generation = config::generation();
        let cfg = config::snapshot();
        Ok(Self {
            dev,
            protocol,
            steam_assembler: SteamPacketAssembler::default(),
            steam_state: SteamRawState::default(),
            steam_epoch: Instant::now(),
            last_lizard_refresh: Instant::now(),
            last_imu_refresh: Instant::now(),
            gyro_map: cfg.gyro,
            accel_map: cfg.accel,
            gyro_sensitivity: cfg.effective_gyro_sensitivity(),
            gyro_cal: GyroCalibration::new(),
            auto_calibrate: cfg.auto_calibrate,
            last_imu_ts_us: None,
            cfg_generation,
            interface_number: info.interface_number(),
            product_id: info.product_id(),
            controller: controller_info(info),
            input_reports_seen: 0,
        })
    }

    pub fn read_one(&mut self, timeout_ms: i32) -> Result<Option<ControllerState>, String> {
        if self.last_lizard_refresh.elapsed() >= LIZARD_REFRESH_INTERVAL {
            match self.protocol {
                Protocol::Triton => {
                    let lizard = build_set_setting_report(SETTING_LIZARD_MODE, LIZARD_MODE_OFF);
                    let _ = self.dev.send_feature_report(&lizard);
                }
                Protocol::Steam2015 { bluetooth } => {
                    let _ = set_steam_2015_input_mode(&self.dev, bluetooth);
                }
            }
            self.last_lizard_refresh = Instant::now();
        }
        if self.last_imu_refresh.elapsed() >= IMU_REFRESH_INTERVAL {
            match self.protocol {
                Protocol::Triton => {
                    let imu =
                        build_set_setting_report(SETTING_IMU_MODE, GYRO_MODE_RAW_ACCEL_AND_GYRO);
                    let _ = self.dev.send_feature_report(&imu);
                }
                Protocol::Steam2015 { bluetooth } => {
                    let _ = enable_steam_2015_imu(&self.dev, bluetooth);
                }
            }
            self.last_imu_refresh = Instant::now();
        }
        let live_generation = config::generation();
        if live_generation != self.cfg_generation {
            let cfg = config::snapshot();
            self.gyro_map = cfg.gyro;
            self.accel_map = cfg.accel;
            self.gyro_sensitivity = cfg.effective_gyro_sensitivity();
            self.auto_calibrate = cfg.auto_calibrate;
            self.cfg_generation = live_generation;
            // Bias is estimated in the post-mapping frame; a remap invalidates it.
            // Toggling auto-calibrate also resets so re-enabling starts fresh.
            self.gyro_cal.reset();
            self.last_imu_ts_us = None;
        }
        let mut buf = [0u8; INPUT_REPORT_BYTES];
        match self.dev.read_timeout(&mut buf, timeout_ms) {
            Ok(0) => Ok(None),
            Ok(n) => {
                self.input_reports_seen = self.input_reports_seen.saturating_add(1);
                let mut state = match self.protocol {
                    Protocol::Triton => {
                        let id = buf[0];
                        if id != TRITON_REPORT_STATE && id != TRITON_REPORT_STATE_BLE {
                            return Ok(None);
                        }
                        let Some(state) = parse_state(&buf[1..n], &self.gyro_map, &self.accel_map)
                        else {
                            return Ok(None);
                        };
                        state
                    }
                    Protocol::Steam2015 { bluetooth: true } => {
                        let Some(packet_len) = self.steam_assembler.push(&buf[..n])? else {
                            return Ok(None);
                        };
                        let packet = &self.steam_assembler.buffer[..packet_len];
                        if update_steam_ble_state(packet, &mut self.steam_state).is_none()
                            && update_steam_full_state(packet, &mut self.steam_state).is_none()
                        {
                            return Ok(None);
                        }
                        steam_controller_state(
                            &self.steam_state,
                            self.steam_epoch.elapsed().as_micros() as u32,
                            &self.gyro_map,
                            &self.accel_map,
                        )
                    }
                    Protocol::Steam2015 { bluetooth: false } => {
                        if update_steam_full_state(&buf[..n], &mut self.steam_state).is_none() {
                            return Ok(None);
                        }
                        steam_controller_state(
                            &self.steam_state,
                            self.steam_epoch.elapsed().as_micros() as u32,
                            &self.gyro_map,
                            &self.accel_map,
                        )
                    }
                };

                let dt = match self.last_imu_ts_us {
                    Some(prev) => {
                        let delta = state.imu.timestamp_us.wrapping_sub(prev);
                        // Clamp at 100 ms — a longer gap means we lost the
                        // stream (Steam took the device, reopen, etc.) and
                        // pretending it's one big step is worse than
                        // skipping the update.
                        (delta as f32 / 1_000_000.0).clamp(0.0, 0.1)
                    }
                    None => 0.0,
                };
                self.last_imu_ts_us = Some(state.imu.timestamp_us);
                if self.auto_calibrate {
                    state.imu.gyro_dps =
                        self.gyro_cal
                            .correct(state.imu.gyro_dps, state.imu.accel_g, dt);
                }
                if self.gyro_sensitivity != 1.0 {
                    for v in &mut state.imu.gyro_dps {
                        *v *= self.gyro_sensitivity;
                    }
                }
                stats::publish_calibration(stats::CalibrationSection {
                    active: self.auto_calibrate,
                    steady: self.gyro_cal.is_steady(),
                    confidence: self.gyro_cal.confidence(),
                });
                Ok(Some(state))
            }
            Err(e) => Err(format!("read: {e}")),
        }
    }

    pub fn recalibrate(&mut self) {
        self.gyro_cal.reset();
        self.last_imu_ts_us = None;
    }
}

impl Drop for OpenSlot {
    fn drop(&mut self) {
        if let Protocol::Steam2015 { bluetooth } = self.protocol {
            restore_steam_2015(&self.dev, bluetooth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_map() -> config::AxisMap {
        config::AxisMap {
            x: config::Axis::new(0, false),
            y: config::Axis::new(1, false),
            z: config::Axis::new(2, false),
        }
    }

    fn build_imu_payload(ts: u32, accel_raw: [i16; 3], gyro_raw: [i16; 3]) -> Vec<u8> {
        let mut p = vec![0u8; 45];
        p[29..33].copy_from_slice(&ts.to_le_bytes());
        for (i, v) in accel_raw.iter().enumerate() {
            p[33 + i * 2..35 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in gyro_raw.iter().enumerate() {
            p[39 + i * 2..41 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        p
    }

    #[test]
    fn parse_imu_rejects_short_payload() {
        assert!(parse_imu(&[0u8; 44], &identity_map(), &identity_map()).is_none());
    }

    #[test]
    fn parse_imu_decodes_full_scale_values() {
        let payload = build_imu_payload(0x1234_5678, [16384, 0, -16384], [16384, -16384, 0]);
        let s = parse_imu(&payload, &identity_map(), &identity_map()).unwrap();
        assert_eq!(s.timestamp_us, 0x1234_5678);
        assert!((s.accel_g[0] - 1.0).abs() < 1e-4);
        assert!(s.accel_g[1].abs() < 1e-4);
        assert!((s.accel_g[2] + 1.0).abs() < 1e-4);
        assert!((s.gyro_dps[0] - 1000.0).abs() < 1e-2);
        assert!((s.gyro_dps[1] + 1000.0).abs() < 1e-2);
        assert!(s.gyro_dps[2].abs() < 1e-2);
    }

    #[test]
    fn parse_imu_applies_axis_mapping() {
        let payload = build_imu_payload(0, [1000, 2000, 3000], [100, 200, 300]);
        let swap_xy = config::AxisMap {
            x: config::Axis::new(1, false),
            y: config::Axis::new(0, true),
            z: config::Axis::new(2, false),
        };
        let direct = parse_imu(&payload, &identity_map(), &identity_map()).unwrap();
        let mapped = parse_imu(&payload, &swap_xy, &identity_map()).unwrap();
        assert!((mapped.gyro_dps[0] - direct.gyro_dps[1]).abs() < 1e-6);
        assert!((mapped.gyro_dps[1] + direct.gyro_dps[0]).abs() < 1e-6);
        assert!((mapped.gyro_dps[2] - direct.gyro_dps[2]).abs() < 1e-6);
    }

    #[test]
    fn parse_state_decodes_buttons_sticks_triggers() {
        let mut p = build_imu_payload(0x1111_2222, [1, 2, 3], [4, 5, 6]);
        p[1..5].copy_from_slice(&(button::A | button::DPAD_LEFT | button::STEAM).to_le_bytes());
        p[5..7].copy_from_slice(&1234u16.to_le_bytes());
        p[7..9].copy_from_slice(&31000u16.to_le_bytes());
        p[9..11].copy_from_slice(&(-100i16).to_le_bytes());
        p[11..13].copy_from_slice(&200i16.to_le_bytes());
        p[13..15].copy_from_slice(&(-300i16).to_le_bytes());
        p[15..17].copy_from_slice(&400i16.to_le_bytes());
        let s = parse_state(&p, &identity_map(), &identity_map()).unwrap();
        assert_eq!(s.buttons, button::A | button::DPAD_LEFT | button::STEAM);
        assert_eq!(s.trigger_left, 1234);
        assert_eq!(s.trigger_right, 31000);
        assert_eq!(s.left_stick, [-100, 200]);
        assert_eq!(s.right_stick, [-300, 400]);
        assert_eq!(s.imu.timestamp_us, 0x1111_2222);
    }

    #[test]
    fn parse_state_rejects_short_payload() {
        assert!(parse_state(&[0u8; 30], &identity_map(), &identity_map()).is_none());
    }

    #[test]
    fn steam_ble_assembler_reassembles_segmented_packets() {
        let mut assembler = SteamPacketAssembler::default();
        let mut first = [0u8; STEAM_BLE_SEGMENT_BYTES];
        first[0] = STEAM_BLE_REPORT_ID;
        first[1] = STEAM_BLE_SEGMENT_DATA;
        first[2..].fill(0x11);
        assert_eq!(assembler.push(&first).unwrap(), None);

        let mut last = [0u8; STEAM_BLE_SEGMENT_BYTES];
        last[0] = STEAM_BLE_REPORT_ID;
        last[1] = STEAM_BLE_SEGMENT_DATA | STEAM_BLE_SEGMENT_LAST | 1;
        last[2..].fill(0x22);
        assert_eq!(assembler.push(&last).unwrap(), Some(36));
        assert_eq!(&assembler.buffer[..18], &[0x11; 18]);
        assert_eq!(&assembler.buffer[18..36], &[0x22; 18]);
    }

    #[test]
    fn steam_ble_state_decodes_sdl_chunks() {
        let chunks = STEAM_BLE_BUTTON_CHUNK_1
            | STEAM_BLE_BUTTON_CHUNK_2
            | STEAM_BLE_LEFT_JOYSTICK_CHUNK
            | STEAM_BLE_IMU_ACCEL_CHUNK
            | STEAM_BLE_IMU_GYRO_CHUNK;
        let mut packet = vec![(chunks as u8 & 0xF0) | STEAM_BLE_STATE, (chunks >> 8) as u8];
        packet.extend_from_slice(&[0x80, 0x10, 0x00]); // A + Back
        packet.extend_from_slice(&[100, 200]);
        for value in [123i16, -456, 16384, 0, -16384, 16384, -8192, 0] {
            packet.extend_from_slice(&value.to_le_bytes());
        }

        let mut raw = SteamRawState::default();
        update_steam_ble_state(&packet, &mut raw).unwrap();
        let state = steam_controller_state(&raw, 123_456, &identity_map(), &identity_map());
        assert_eq!(state.buttons, button::A | button::VIEW);
        assert_eq!(state.left_stick, [123, 455]);
        assert_eq!(state.trigger_left, steam_trigger(100));
        assert_eq!(state.trigger_right, steam_trigger(200));
        assert_eq!(state.imu.timestamp_us, 123_456);
        assert!((state.imu.accel_g[0] - 1.0).abs() < 1e-4);
        assert!((state.imu.accel_g[2] + 1.0).abs() < 1e-4);
        assert!((state.imu.gyro_dps[0] - 1000.0).abs() < 1e-2);
        assert!((state.imu.gyro_dps[1] - 500.0).abs() < 1e-2);
    }
}
