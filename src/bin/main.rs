/*
 * GPS NMEA Sentence Reader for ESP32-C3
 *
 * This program reads NMEA sentences from a GPS module via UART and displays them.
 * NMEA (National Marine Electronics Association) sentences are standardized GPS data
 * messages that contain location, time, and satellite information.
 *
 * How it works:
 * 1. GPS module sends continuous stream of NMEA sentences via UART
 * 2. Each sentence is a line of text ending with \r\n (carriage return + newline)
 * 3. We read bytes one at a time and buffer them until we find a line ending
 * 4. When a complete sentence is received, we convert it to a string and print it
 *
 * Example NMEA sentences:
 * $GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47
 * $GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A
 */

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt;
use embassy_executor::Spawner;
use embassy_time;
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::uart::{Config, Uart};
use esp_hal::Async;
use esp_println as _;
use heapless::Vec;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Initialize the ESP32-C3 with maximum CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize embassy
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // Configure GPIO pins for UART communication with GPS module
    // TX pin sends data to GPS (not used in this read-only example)
    // RX pin receives NMEA sentences from GPS
    let tx_pin = peripherals.GPIO4; // Connect to GPS RX pin
    let rx_pin = peripherals.GPIO5; // Connect to GPS TX pin

    // Configure UART with 9600 baud rate (standard for most GPS modules)
    let config = Config::default().with_baudrate(9600);
    let uart = Uart::new(peripherals.UART1, config)
        .expect("UART initialization failed")
        .with_rx(rx_pin)
        .with_tx(tx_pin)
        .into_async(); // Convert to async mode

    // Spawn the GPS reading task
    spawner.spawn(gps_task(uart)).unwrap();

    // Keep the main task alive
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn gps_task(mut uart: Uart<'static, Async>) {
    // Buffer to store incoming NMEA sentence bytes
    let mut sentence = [0u8; 128]; // 128 bytes should be enough for most NMEA sentences
    let mut idx = 0; // Current position in the sentence buffer

    // Main loop: continuously read and parse GPS data
    // GPS modules send a continuous stream of NMEA sentences like:
    // "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n"
    // We need to read this character by character and assemble complete sentences
    loop {
        // Read one byte at a time from the UART
        // GPS sends data at 9600 baud, which is about 960 characters per second
        let mut buffer = [0u8; 1]; // Single byte buffer for reading
        match uart.read_async(&mut buffer).await {
            Ok(_) => {
                // Successfully read a byte from GPS module
                let byte = buffer[0];

                // Store the byte in our sentence buffer if there's space
                // We're building up the sentence character by character
                if idx < sentence.len() {
                    sentence[idx] = byte;
                    idx += 1;
                }

                // Check if we've reached the end of an NMEA sentence
                // NMEA sentences end with \r\n (carriage return + newline)
                // or if our buffer is full (safety check)
                if byte == b'\n' || byte == b'\r' || idx == sentence.len() {
                    // Only process sentences that contain actual data (more than just newline)
                    if idx > 1 {
                        // Convert the byte array to a UTF-8 string
                        // This handles the conversion from raw bytes to readable text
                        if let Ok(s) = core::str::from_utf8(&sentence[..idx]) {
                            // Print the complete NMEA sentence (trimmed of whitespace)
                            // Example output: "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47"
                            // Parse and print extracted GPS data
                            let sentence_str = s.trim();
                            if sentence_str.starts_with("$GPGGA") {
                                if let Some((lat, lon, sats)) = parse_gga(sentence_str) {
                                    defmt::info!(
                                        "Lat: {}, Lon: {}, Satellites: {}",
                                        lat,
                                        lon,
                                        sats
                                    );
                                }
                            } else if sentence_str.starts_with("$GPRMC") {
                                if let Some((lat, lon, speed, heading)) = parse_rmc(sentence_str) {
                                    defmt::info!(
                                        "Lat: {}, Lon: {}, Speed: {} knots, Heading: {}°",
                                        lat,
                                        lon,
                                        speed,
                                        heading
                                    );
                                }
                            }
                        }
                    }
                    // Reset buffer index to start collecting the next sentence
                    // This prepares us to receive the next NMEA sentence
                    idx = 0;
                }
            }
            Err(e) => {
                // Handle UART errors (signal glitches, framing errors, etc.)
                // These can occur due to electrical interference or baud rate mismatches
                defmt::error!("UART error: {:?}", e);
            }
        }
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}

// Helper: Convert NMEA lat/lon to decimal degrees
fn nmea_to_decimal(coord: &str, dir: &str) -> Option<f32> {
    if coord.len() < 4 {
        return None;
    }
    let (degrees, minutes) = if coord.contains('.') {
        let split = coord.find('.')?;
        let deg_len = split - 2;
        let degrees = &coord[..deg_len];
        let minutes = &coord[deg_len..];
        (degrees, minutes)
    } else {
        let deg_len = coord.len() - 2;
        let degrees = &coord[..deg_len];
        let minutes = &coord[deg_len..];
        (degrees, minutes)
    };
    let deg: f32 = degrees.parse().ok()?;
    let min: f32 = minutes.parse().ok()?;
    let mut val = deg + (min / 60.0);
    if dir == "S" || dir == "W" {
        val = -val;
    }
    Some(val)
}

// Parse $GPGGA sentence: lat, lon, satellites
fn parse_gga(sentence: &str) -> Option<(f32, f32, u8)> {
    let mut fields: Vec<&str, 16> = Vec::new();
    for field in sentence.split(',') {
        let _ = fields.push(field);
    }
    if fields.len() < 8 {
        return None;
    }
    let lat = nmea_to_decimal(fields[2], fields[3])?;
    let lon = nmea_to_decimal(fields[4], fields[5])?;
    let sats: u8 = fields[7].parse().ok()?;
    Some((lat, lon, sats))
}

// Parse $GPRMC sentence: lat, lon, speed, heading
fn parse_rmc(sentence: &str) -> Option<(f32, f32, f32, f32)> {
    let mut fields: Vec<&str, 16> = Vec::new();
    for field in sentence.split(',') {
        let _ = fields.push(field);
    }
    if fields.len() < 9 {
        return None;
    }
    let lat = nmea_to_decimal(fields[3], fields[4])?;
    let lon = nmea_to_decimal(fields[5], fields[6])?;
    let speed: f32 = fields[7].parse().ok()?; // knots
    let heading: f32 = fields[8].parse().ok()?; // degrees
    Some((lat, lon, speed, heading))
}
