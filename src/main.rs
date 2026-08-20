// GUI mode should never flash a second CMD window on Windows, including from
// a debug build. `--headless` and `--probe` explicitly attach or create one
// below so their diagnostics remain visible.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod autostart;
mod config;
mod dsu;
mod gyro_calibration;
mod probe;
mod stats;
mod triton;
mod ui;

use hidapi::HidApi;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

const SAMPLE_QUEUE_LEN: usize = 64;
/// How often the scanner thread re-enumerates HID devices looking for new controllers.
const SCAN_INTERVAL: Duration = Duration::from_secs(1);
/// First retry delay for an interface that refused to open, doubling up to the maximum.
const OPEN_RETRY_MIN: Duration = Duration::from_secs(1);
const OPEN_RETRY_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Gui { start_minimized: bool },
    Headless,
    Probe,
}

fn parse_args() -> Mode {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(args: impl Iterator<Item = String>) -> Mode {
    for arg in args {
        match arg.as_str() {
            "--probe" | "-p" => return Mode::Probe,
            "--headless" | "-H" => return Mode::Headless,
            "--tray" | "--minimized" => {
                return Mode::Gui {
                    start_minimized: true,
                };
            }
            "--gui" => {
                return Mode::Gui {
                    start_minimized: false,
                };
            }
            _ => {}
        }
    }
    Mode::Gui {
        start_minimized: false,
    }
}

#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};
    // SAFETY: AttachConsole/AllocConsole take no caller-supplied pointers; a failed
    // AttachConsole (no parent console, or already attached) is detected via its return
    // value, and AllocConsole's failure (e.g. console already present) is harmless here.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
        }
    }
}

#[cfg(not(windows))]
fn attach_console() {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match parse_args() {
        Mode::Probe => {
            attach_console();
            probe::run()
        }
        Mode::Headless => {
            attach_console();
            run_server(None)
        }
        Mode::Gui { start_minimized } => run_server(Some(start_minimized)),
    }
}

fn run_server(gui_start_minimized: Option<bool>) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_or_create();
    let dsu_port = cfg.port;
    let dsu_expose = cfg.expose_to_network;
    config::install(cfg);

    let dsu_wants_device = Arc::new(AtomicBool::new(false));
    let ui_wants_device = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = sync_channel::<triton::DeviceEvent>(SAMPLE_QUEUE_LEN);

    // Device discovery runs on its own thread. Enumerating the HID tree and opening
    // interfaces are both blocking calls that can take hundreds of milliseconds on a
    // busy system, so they must never share a thread with the controller read loop.
    let owned_paths: OwnedPaths = Arc::new(Mutex::new(HashSet::new()));
    let (dev_tx, dev_rx) = channel::<OpenedDevice>();

    let scanner_handle = {
        let dsu_wants = dsu_wants_device.clone();
        let ui_wants = ui_wants_device.clone();
        let shutdown = shutdown.clone();
        let owned = owned_paths.clone();
        thread::Builder::new()
            .name("controller-scanner".into())
            .spawn(move || run_scanner_thread(dsu_wants, ui_wants, shutdown, owned, dev_tx))?
    };

    let device_handle = {
        let dsu_wants = dsu_wants_device.clone();
        let ui_wants = ui_wants_device.clone();
        let shutdown = shutdown.clone();
        let owned = owned_paths.clone();
        thread::Builder::new()
            .name("controller-reader".into())
            .spawn(move || run_device_thread(dsu_wants, ui_wants, shutdown, tx, dev_rx, owned))?
    };

    let server_handle = {
        let dsu_wants = dsu_wants_device.clone();
        let shutdown = shutdown.clone();
        thread::Builder::new()
            .name("dsu-server".into())
            .spawn(move || -> std::io::Result<()> {
                let mut server = dsu::Server::bind(dsu_port, dsu_expose, dsu_wants, shutdown, rx)?;
                eprintln!(
                    "sc2dsu DSU server listening on {}  (server id 0x{:08X})",
                    server.local_addr()?,
                    server.server_id()
                );
                eprintln!("waiting for client activity before opening controllers ...");
                server.run()
            })?
    };

    match gui_start_minimized {
        Some(start_minimized) => {
            ui::run(shutdown.clone(), ui_wants_device.clone(), start_minimized)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        }
        None => {
            let _ = server_handle.join();
        }
    }

    shutdown.store(true, Ordering::Relaxed);
    let _ = device_handle.join();
    let _ = scanner_handle.join();
    Ok(())
}

/// A controller the scanner thread opened, on its way to the polling thread.
struct OpenedDevice {
    path: Vec<u8>,
    device: triton::OpenSlot,
}

/// Device paths the polling thread currently holds, so the scanner will not reopen them.
type OwnedPaths = Arc<Mutex<HashSet<Vec<u8>>>>;

/// A lock on the owned-path set that survives a panic in the other thread; the set is
/// plain data, so inheriting it after a poisoning is safe and better than dying too.
fn lock_paths(owned: &OwnedPaths) -> MutexGuard<'_, HashSet<Vec<u8>>> {
    owned
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn release_path(owned: &OwnedPaths, path: &[u8]) {
    lock_paths(owned).remove(path);
}

/// Sleeps in short steps so shutdown does not have to wait out a whole scan interval.
fn sleep_until(shutdown: &AtomicBool, total: Duration) {
    const STEP: Duration = Duration::from_millis(50);
    let deadline = Instant::now() + total;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        thread::sleep(remaining.min(STEP));
    }
}

/// An interface that would not open, and when it is worth trying again.
struct OpenFailure {
    attempts: u32,
    next_attempt: Instant,
}

/// Enumerates HID devices and opens new controllers, handing them to the polling thread.
///
/// `HidApi::refresh_devices` walks the whole HID tree and `OpenSlot::open` issues feature
/// reports to hardware; both block for as long as the OS takes. Keeping them here means a
/// slow enumeration delays only the next discovery, never a live controller's reads.
fn run_scanner_thread(
    dsu_wants: Arc<AtomicBool>,
    ui_wants: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    owned: OwnedPaths,
    dev_tx: Sender<OpenedDevice>,
) {
    let want_device = || dsu_wants.load(Ordering::Relaxed) || ui_wants.load(Ordering::Relaxed);

    let mut api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("controller: HidApi init failed: {e}");
            return;
        }
    };

    // Multi-slot receivers such as the Proteus Puck expose one interface per slot and the
    // unpaired ones fail to open every time. Retrying those once a second burns a blocking
    // open apiece and floods the log, so each failing path backs off exponentially.
    let mut retry: HashMap<Vec<u8>, OpenFailure> = HashMap::new();

    while !shutdown.load(Ordering::Relaxed) {
        if !want_device() {
            retry.clear();
            sleep_until(&shutdown, Duration::from_millis(200));
            continue;
        }

        if let Err(e) = api.refresh_devices() {
            eprintln!("controller: refresh_devices failed ({e}); rebuilding HidApi");
            match HidApi::new() {
                Ok(a) => api = a,
                Err(e) => {
                    eprintln!("controller: HidApi re-init failed: {e}; backing off");
                    sleep_until(&shutdown, Duration::from_secs(1));
                    continue;
                }
            }
        }

        let mut present = HashSet::new();
        for info in triton::list_candidates(&api) {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let path = info.path().to_bytes().to_vec();
            present.insert(path.clone());

            if lock_paths(&owned).contains(&path) {
                continue;
            }
            if let Some(failure) = retry.get(&path)
                && Instant::now() < failure.next_attempt
            {
                continue;
            }

            match triton::OpenSlot::open(&api, &info) {
                Ok(device) => {
                    retry.remove(&path);
                    eprintln!(
                        "controller: opened iface {} (PID {:04X} {})",
                        device.interface_number,
                        device.product_id,
                        triton::pid_label(device.product_id),
                    );
                    lock_paths(&owned).insert(path.clone());
                    if dev_tx.send(OpenedDevice { path, device }).is_err() {
                        return; // Polling thread is gone; nothing left to scan for.
                    }
                }
                Err(e) => {
                    let failure = retry.entry(path).or_insert(OpenFailure {
                        attempts: 0,
                        next_attempt: Instant::now(),
                    });
                    failure.attempts += 1;
                    let delay = OPEN_RETRY_MIN
                        .saturating_mul(1u32 << failure.attempts.saturating_sub(1).min(6))
                        .min(OPEN_RETRY_MAX);
                    failure.next_attempt = Instant::now() + delay;
                    eprintln!(
                        "controller: open iface {} (PID {:04X}) failed: {e}; retrying in {}s",
                        info.interface_number(),
                        info.product_id(),
                        delay.as_secs()
                    );
                }
            }
        }

        // An interface that vanished from enumeration (unplugged, or re-paired to a slot)
        // gets a clean slate rather than staying stuck at a long backoff.
        retry.retain(|path, _| present.contains(path));

        sleep_until(&shutdown, SCAN_INTERVAL);
    }
}

/// Polls every open controller. Does no enumeration and no opening, so nothing here can
/// block on the OS for longer than a single HID read.
fn run_device_thread(
    dsu_wants: Arc<AtomicBool>,
    ui_wants: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    tx: SyncSender<triton::DeviceEvent>,
    dev_rx: Receiver<OpenedDevice>,
    owned: OwnedPaths,
) {
    let want_device = || dsu_wants.load(Ordering::Relaxed) || ui_wants.load(Ordering::Relaxed);

    let mut controllers = Vec::<ManagedController>::new();

    while !shutdown.load(Ordering::Relaxed) {
        if !want_device() {
            disconnect_all(&mut controllers, &tx, &owned);
            // Anything the scanner opened just before the request went away.
            while let Ok(pending) = dev_rx.try_recv() {
                release_path(&owned, &pending.path);
            }
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        while let Ok(pending) = dev_rx.try_recv() {
            controllers.push(ManagedController {
                path: pending.path,
                device: pending.device,
                dsu_slot: None,
                last_sample_at: Instant::now(),
                consecutive_errors: 0,
                last_imu_timestamp: None,
                stale_samples: 0,
            });
        }

        if stats::RECALIBRATE_REQUEST.swap(false, Ordering::Relaxed) {
            for controller in &mut controllers {
                controller.device.recalibrate();
            }
        }
        let requested_slot = stats::RECALIBRATE_SLOT_REQUEST.swap(0, Ordering::Relaxed);
        if requested_slot != 0 {
            let slot = requested_slot - 1;
            for controller in &mut controllers {
                if controller.dsu_slot == Some(slot) {
                    controller.device.recalibrate();
                }
            }
        }

        poll_controllers(&mut controllers, &tx, &owned);
        thread::sleep(Duration::from_millis(2));
    }

    disconnect_all(&mut controllers, &tx, &owned);
}

struct ManagedController {
    path: Vec<u8>,
    device: triton::OpenSlot,
    dsu_slot: Option<u8>,
    last_sample_at: Instant,
    consecutive_errors: u32,
    last_imu_timestamp: Option<u32>,
    stale_samples: u32,
}

/// Resolve manual preferences first, then give every unpinned controller the
/// first remaining DSU slot.  This preserves discovery-order behaviour until a
/// user explicitly pins a controller in the dashboard.
fn desired_slot_assignments(
    controllers: &[ManagedController],
    cfg: &config::Config,
) -> Vec<Option<u8>> {
    let mut desired = vec![None; controllers.len()];
    let mut occupied = [false; triton::MAX_CONTROLLERS];

    for (index, controller) in controllers.iter().enumerate() {
        let Some(slot) = cfg.slot_for_controller(controller.device.controller.id) else {
            continue;
        };
        let slot_index = usize::from(slot);
        if !occupied[slot_index] {
            desired[index] = Some(slot);
            occupied[slot_index] = true;
        }
    }

    for assignment in &mut desired {
        if assignment.is_none()
            && let Some(slot) = occupied.iter().position(|used| !used)
        {
            *assignment = Some(slot as u8);
            occupied[slot] = true;
        }
    }
    desired
}

fn sync_slot_assignments(
    controllers: &mut [ManagedController],
    tx: &SyncSender<triton::DeviceEvent>,
) {
    let desired = desired_slot_assignments(controllers, &config::snapshot());
    for (controller, desired_slot) in controllers.iter_mut().zip(desired) {
        if controller.dsu_slot == desired_slot {
            continue;
        }
        if let Some(old_slot) = controller.dsu_slot {
            let _ = tx.send(triton::DeviceEvent::Disconnected { slot: old_slot });
        }
        controller.dsu_slot = desired_slot;
        if let Some(slot) = desired_slot {
            eprintln!(
                "controller: routed PID {:04X} iface {} to DSU slot {}",
                controller.device.product_id, controller.device.interface_number, slot
            );
            let _ = tx.send(triton::DeviceEvent::Connected {
                slot,
                controller: controller.device.controller,
            });
        }
    }
}

fn poll_controllers(
    controllers: &mut Vec<ManagedController>,
    tx: &SyncSender<triton::DeviceEvent>,
    owned: &OwnedPaths,
) {
    const SILENCE_REOPEN_MS: u128 = 2000;
    const STALE_THRESHOLD: u32 = 100;

    sync_slot_assignments(controllers, tx);

    let mut index = 0;
    while index < controllers.len() {
        let mut remove = false;
        match controllers[index].device.read_one(0) {
            Ok(Some(sample)) => {
                controllers[index].consecutive_errors = 0;
                controllers[index].last_sample_at = Instant::now();
                let fresh_sample =
                    controllers[index].last_imu_timestamp != Some(sample.imu.timestamp_us);
                if !fresh_sample {
                    controllers[index].stale_samples += 1;
                    if controllers[index].stale_samples >= STALE_THRESHOLD {
                        eprintln!(
                            "controller: IMU timestamp frozen for {STALE_THRESHOLD} samples; reopening interface"
                        );
                        remove = true;
                    }
                } else {
                    controllers[index].last_imu_timestamp = Some(sample.imu.timestamp_us);
                    controllers[index].stale_samples = 0;
                }
                if fresh_sample && let Some(slot) = controllers[index].dsu_slot {
                    let _ = tx.try_send(triton::DeviceEvent::Sample {
                        slot,
                        state: sample,
                    });
                }
            }
            Ok(None) => {
                controllers[index].consecutive_errors = 0;
                if controllers[index].last_sample_at.elapsed().as_millis() >= SILENCE_REOPEN_MS {
                    remove = true;
                }
            }
            Err(e) => {
                controllers[index].consecutive_errors += 1;
                if controllers[index].consecutive_errors >= 5 {
                    eprintln!("controller: 5 consecutive read errors ({e}); reopening interface");
                    remove = true;
                }
            }
        }

        if remove {
            let controller = controllers.remove(index);
            // Hand the path back so the scanner can reopen this interface.
            release_path(owned, &controller.path);
            if let Some(slot) = controller.dsu_slot {
                let _ = tx.send(triton::DeviceEvent::Disconnected { slot });
                eprintln!("controller: DSU slot {slot} disconnected");
            }
        } else {
            index += 1;
        }
    }
}

fn disconnect_all(
    controllers: &mut Vec<ManagedController>,
    tx: &SyncSender<triton::DeviceEvent>,
    owned: &OwnedPaths,
) {
    for controller in controllers.drain(..) {
        release_path(owned, &controller.path);
        if let Some(slot) = controller.dsu_slot {
            let _ = tx.send(triton::DeviceEvent::Disconnected { slot });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Mode {
        parse_args_from(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn parse_args_defaults_to_visible_gui() {
        assert_eq!(
            parse(&[]),
            Mode::Gui {
                start_minimized: false
            }
        );
        assert_eq!(
            parse(&["some-positional-arg"]),
            Mode::Gui {
                start_minimized: false
            }
        );
    }

    #[test]
    fn parse_args_recognizes_each_mode() {
        assert_eq!(parse(&["--probe"]), Mode::Probe);
        assert_eq!(parse(&["-p"]), Mode::Probe);
        assert_eq!(parse(&["--headless"]), Mode::Headless);
        assert_eq!(parse(&["-H"]), Mode::Headless);
        assert_eq!(
            parse(&["--gui"]),
            Mode::Gui {
                start_minimized: false
            }
        );
        assert_eq!(
            parse(&["--tray"]),
            Mode::Gui {
                start_minimized: true
            }
        );
        assert_eq!(
            parse(&["--minimized"]),
            Mode::Gui {
                start_minimized: true
            }
        );
    }

    #[test]
    fn parse_args_uses_first_recognized_flag() {
        assert_eq!(
            parse(&["--gui", "--probe"]),
            Mode::Gui {
                start_minimized: false
            }
        );
    }
}
