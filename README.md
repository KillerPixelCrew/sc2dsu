# SC2DSU

Cemuhook DSU server for the original 2015 and 2026 Steam Controllers. It forwards motion, buttons, sticks, pads, and triggers to emulators such as Cemu, Eden, Citra, and Ryujinx on `127.0.0.1:26760`.

Download the Windows or Linux binary from [Releases](https://github.com/NightHammer1000/sc2dsu/releases) and run it, then point your emulator at `127.0.0.1:26760`.

Up to four connected controllers are exposed as DSU slots 0–3 in discovery order. Per-slot and per-MAC DSU subscriptions are both supported, so local multiplayer clients only receive the controller slots they request.

If an axis is wrong, swap the source or flip invert in the settings window. Saved live; takes effect on the next IMU sample. Config lives at `%APPDATA%\sc2dsu\config.toml` on Windows or `$XDG_CONFIG_HOME/sc2dsu/config.toml` (normally `~/.config/sc2dsu/config.toml`) on Linux.

The settings window, live status, tray controls, autostart, calibration controls, and start-minimized/close-to-tray behavior are available on both Windows and Linux.

## Linux

The Linux binary is portable: it links only against the C runtime every distribution
already has (`libc`, `libm`, `libgcc_s`), and loads X11, Wayland and OpenGL at runtime, so
there is nothing to install. Download it, `chmod +x`, run it. Release builds target glibc
2.35, which covers Debian 12, Ubuntu 22.04 and newer, SteamOS 3.x, and current Arch and
Fedora.

A source build needs only a Rust toolchain -- no development packages:

```sh
cargo build --release
```

If the controller cannot be opened, add a udev access rule and reconnect it:

```sh
echo 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", TAG+="uaccess"' | \
  sudo tee /etc/udev/rules.d/70-sc2dsu.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Use `sc2dsu --probe` to verify HID access.

Run modes: `sc2dsu` (GUI + server), `sc2dsu --tray` (start hidden), `sc2dsu --headless` (server only, logs to stderr), `sc2dsu --probe` (enumerate Valve HIDs and dump 3 s of decoded IMU).

### GUI launcher without a terminal

The provided desktop entry has `Terminal=false`, so launching SC2DSU from the
application menu does not open a terminal window. After installing the binary
at `/usr/local/bin/sc2dsu`, install the launcher with:

```sh
sudo install -Dm755 target/release/sc2dsu /usr/local/bin/sc2dsu
sudo install -Dm644 packaging/linux/sc2dsu.desktop /usr/local/share/applications/sc2dsu.desktop
```

Running `sc2dsu` manually in a terminal intentionally keeps its diagnostic
output attached to that terminal. Use `sc2dsu --headless` with the systemd
service below when no GUI is wanted.

### Headless systemd service

For a machine that should serve DSU without a desktop session, use the provided
user service. First install the release binary at `~/.local/bin/sc2dsu` (or
edit `ExecStart` in the example), then install and start the service:

```sh
install -Dm644 packaging/systemd/sc2dsu.service ~/.config/systemd/user/sc2dsu.service
systemctl --user daemon-reload
systemctl --user enable --now sc2dsu
```

Check its output with `journalctl --user -u sc2dsu -f`. The service uses
`--headless`, so it does not open an egui window. Ensure the account running
the service has HID access via the udev rule above; a user service is preferred
over running the DSU server as root.

Tested hardware:

- 2015 Steam Controller over Bluetooth (`28DE:1106`)
- 2026 Steam Controller over the Proteus Puck (`28DE:1304`)

The SDL3-listed 2015 wired (`0x1102`), BLE (`0x1105`/`0x1106`), and wireless dongle (`0x1142`) transports are supported. Triton wired (`0x1302`), BLE (`0x1303`), Proteus (`0x1304`), and Nereid (`0x1305`) use the existing Triton path; transports not listed as tested above still need hardware reports.

Build with `cargo build --release`. CI runs formatting, tests, Clippy, and release builds on Windows and Linux.

HID protocol from SDL3 [`SDL_hidapi_steam.c`](https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/SDL_hidapi_steam.c), [`SDL_hidapi_steam_triton.c`](https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/SDL_hidapi_steam_triton.c), and the [steam protocol headers](https://github.com/libsdl-org/SDL/tree/main/src/joystick/hidapi/steam). DSU protocol from [v1993/gcemuhook](https://github.com/v1993/gcemuhook). MIT.

# Notice on SDL Native Emulators
Emulators that support the Controller Nativly like RPCS3 need the Steam Overlay disabled on their Shortcut to stop Steam Input from Injecting and Hiding the Controller.
SDL Native Programms also do not need this Tool to access Gyro. 
**So. No. You dont need this for RPCS3**


# Like it? Found it useful?

You can help fuel my caffeine addiction here:
https://ko-fi.com/nightstorm1000
