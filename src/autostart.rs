use std::io;

const APP_NAME: &str = "SC2DSU";

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::*;
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn is_enabled() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN_KEY)
            .and_then(|key: RegKey| key.get_value::<String, _>(APP_NAME))
            .is_ok()
    }
    pub fn enable() -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
        key.set_value(
            APP_NAME,
            &format!("\"{}\" --tray", std::env::current_exe()?.display()),
        )
    }
    pub fn disable() -> io::Result<()> {
        let key =
            RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
        match key.delete_value(APP_NAME) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::path::PathBuf;
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("autostart/sc2dsu.desktop")
    }
    pub fn is_enabled() -> bool {
        path().is_file()
    }
    pub fn enable() -> io::Result<()> {
        let path = path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let exe = std::env::current_exe()?;
        std::fs::write(
            path,
            format!(
                "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={} --tray\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                shell_quote(&exe.to_string_lossy())
            ),
        )
    }
    pub fn disable() -> io::Result<()> {
        match std::fs::remove_file(path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn shell_quote(value: &str) -> String {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub fn is_enabled() -> bool {
        false
    }
    pub fn enable() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "autostart is not supported on this platform",
        ))
    }
    pub fn disable() -> io::Result<()> {
        Ok(())
    }
}

pub use platform::{disable, enable, is_enabled};
