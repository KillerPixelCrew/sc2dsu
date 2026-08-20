# SC2DSU

Cemuhook DSU server for the original 2015 and 2026 Steam Controllers. It forwards motion, buttons, sticks, pads, and triggers to emulators such as Cemu, Eden, Citra, and Ryujinx on `127.0.0.1:26760`.

Download the Windows or Linux binary from [Releases](https://github.com/NightHammer1000/sc2dsu/releases) and run it, then point your emulator at `127.0.0.1:26760`.

Up to four connected controllers are exposed as DSU slots 0–3 in discovery order. Per-slot and per-MAC DSU subscriptions are both supported, so local multiplayer clients only receive the controller slots they request.

If an axis is wrong, swap the source or flip invert in the settings window. Saved live; takes effect on the next IMU sample. Config lives at `%APPDATA%\sc2dsu\config.toml` on Windows or `$XDG_CONFIG_HOME/sc2dsu/config.toml` (normally `~/.config/sc2dsu/config.toml`) on Linux.

The settings window, live status and motion visualization, tray controls, autostart, calibration controls, and start-minimized/close-to-tray behavior are available on both Windows and Linux.

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

Run modes: `sc2dsu` (GUI + server), `sc2dsu --tray` (start hidden), `sc2dsu --headless` (server only, log to stderr), `sc2dsu --probe` (enumerate Valve HIDs and dump 3 s of decoded IMU).

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
