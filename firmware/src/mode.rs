/// Magic number to indicate WiFi mode boot
const WIFI_MODE_MAGIC: u32 = 0xCAFE_BABE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    GpsMode,
    WifiMode,
}

/// Check RTC STORE0 register to determine boot mode
/// STORE0 is less likely to be used by bootloader than STORE4
pub fn get_boot_mode() -> BootMode {
    let rtc_cntl = unsafe { &*esp_hal::peripherals::LPWR::PTR };
    let value = rtc_cntl.store0().read().bits();

    if value == WIFI_MODE_MAGIC {
        defmt::info!("RTC: WiFi mode magic number detected (0x{:08X})", value);
        BootMode::WifiMode
    } else {
        defmt::info!(
            "RTC: No magic number (0x{:08X}), defaulting to GPS mode",
            value
        );
        BootMode::GpsMode
    }
}

/// Clear the RTC boot mode (called when entering WiFi mode)
pub fn clear_boot_mode() {
    let rtc_cntl = unsafe { &*esp_hal::peripherals::LPWR::PTR };
    rtc_cntl.store0().write(|w| unsafe { w.bits(0) });
    defmt::info!("RTC: Boot mode cleared");
}

/// Set WiFi mode for next boot and trigger software reset
pub fn request_wifi_mode_reboot() {
    defmt::info!("Mode: Setting WiFi mode for next boot...");

    let rtc_cntl = unsafe { &*esp_hal::peripherals::LPWR::PTR };

    // Read current value before writing
    let before = rtc_cntl.store0().read().bits();
    defmt::info!("Mode: STORE0 before write: 0x{:08X}", before);

    rtc_cntl
        .store0()
        .write(|w| unsafe { w.bits(WIFI_MODE_MAGIC) });

    // Read back to verify write
    let after = rtc_cntl.store0().read().bits();
    defmt::info!("Mode: STORE0 after write: 0x{:08X}", after);

    // Small delay to ensure write completes
    for _ in 0..1000 {
        unsafe { core::arch::asm!("nop") };
    }

    defmt::info!("Mode: Rebooting to WiFi mode...");
    esp_hal::system::software_reset();
}

/// Trigger software reset to return to GPS mode (RTC already cleared)
pub fn request_gps_mode_reboot() {
    defmt::info!("Mode: Rebooting to GPS mode...");
    esp_hal::system::software_reset();
}
