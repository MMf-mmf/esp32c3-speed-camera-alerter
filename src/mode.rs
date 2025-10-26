use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SystemMode {
    GpsMode = 0,
    WifiApMode = 1,
}

impl From<u8> for SystemMode {
    fn from(value: u8) -> Self {
        match value {
            0 => SystemMode::GpsMode,
            1 => SystemMode::WifiApMode,
            _ => SystemMode::GpsMode,
        }
    }
}

pub static CURRENT_MODE: AtomicU8 = AtomicU8::new(SystemMode::GpsMode as u8);
pub static MODE_CHANGE_SIGNAL: Signal<CriticalSectionRawMutex, SystemMode> = Signal::new();
pub static WIFI_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn get_mode() -> SystemMode {
    SystemMode::from(CURRENT_MODE.load(Ordering::Relaxed))
}

pub fn set_mode(mode: SystemMode) {
    CURRENT_MODE.store(mode as u8, Ordering::Relaxed);
    MODE_CHANGE_SIGNAL.signal(mode);
}

pub fn request_wifi_mode() {
    esp_println::println!("Mode: Switching to WiFi AP mode");
    WIFI_SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);
    set_mode(SystemMode::WifiApMode);
}

pub fn request_gps_mode() {
    esp_println::println!("Mode: Switching to GPS mode");
    WIFI_SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
    set_mode(SystemMode::GpsMode);
}

pub fn is_wifi_mode() -> bool {
    get_mode() == SystemMode::WifiApMode
}

pub fn is_gps_mode() -> bool {
    get_mode() == SystemMode::GpsMode
}

pub fn is_wifi_shutdown_requested() -> bool {
    WIFI_SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}
