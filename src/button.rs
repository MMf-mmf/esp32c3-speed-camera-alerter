use embassy_time::{Duration, Timer};
use esp_hal::gpio::Input;

const LONG_PRESS_DURATION_MS: u64 = 2000; // 2 seconds

#[embassy_executor::task]
pub async fn button_task(mut button: Input<'static>) {
    esp_println::println!("Button task started - Long press (2s) to toggle WiFi AP mode");

    loop {
        button.wait_for_falling_edge().await;

        let press_start = embassy_time::Instant::now();
        esp_println::println!("Button pressed...");

        let mut long_press = false;
        loop {
            Timer::after(Duration::from_millis(100)).await;

            if button.is_high() {
                let press_duration = embassy_time::Instant::now() - press_start;
                if press_duration.as_millis() >= LONG_PRESS_DURATION_MS {
                    long_press = true;
                }
                break;
            }

            let press_duration = embassy_time::Instant::now() - press_start;
            if press_duration.as_millis() >= LONG_PRESS_DURATION_MS {
                esp_println::println!("Long press detected!");
                long_press = true;
                button.wait_for_rising_edge().await;
                break;
            }
        }

        if long_press {
            match crate::mode::get_mode() {
                crate::mode::SystemMode::GpsMode => {
                    esp_println::println!("Activating WiFi AP mode for OTA updates");
                    crate::mode::request_wifi_mode();
                }
                crate::mode::SystemMode::WifiApMode => {
                    esp_println::println!("Returning to GPS mode");
                    crate::mode::request_gps_mode();
                }
            }
        } else {
            esp_println::println!("Short press ignored - hold for 2s to toggle mode");
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}
