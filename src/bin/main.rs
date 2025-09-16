/*
 * GPS NMEA Sentence Reader with OLED Display for ESP32-C3
 *
 * This program reads NMEA sentences from a GPS module via UART and displays
 * the GPS coordinates and satellite count on an OLED screen.
 */

#![no_std]
#![no_main]

use core::fmt::Write;
use defmt;
use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::ascii::FONT_9X15;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use esp_hal::{time::Rate, Async};
use esp_println as _;
use heapless::{String, Vec};
use ssd1306::mode::DisplayConfigAsync;
use ssd1306::{
    prelude::DisplayRotation, size::DisplaySize128x64, I2CDisplayInterface, Ssd1306Async,
};
use static_cell::StaticCell;

// Shared GPS data structure
#[derive(Clone, Copy)]
struct GpsData {
    lat: f32,
    lon: f32,
    satellites: u8,
    speed: f32,   // Speed in knots
    heading: f32, // Heading in degrees
    valid: bool,
}

impl Default for GpsData {
    fn default() -> Self {
        Self {
            lat: 0.0,
            lon: 0.0,
            satellites: 0,
            speed: 0.0,
            heading: 0.0,
            valid: false,
        }
    }
}

// Global shared GPS data - use a reference that gets initialized
static GPS_DATA_CELL: StaticCell<
    Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, GpsData>,
> = StaticCell::new();
static mut GPS_DATA_REF: Option<
    &'static Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, GpsData>,
> = None;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    // Initialize shared GPS data
    let gps_data_mutex = Mutex::new(GpsData::default());
    let gps_data_ref = GPS_DATA_CELL.init(gps_data_mutex);
    unsafe {
        GPS_DATA_REF = Some(gps_data_ref);
    }

    // Configure UART for GPS
    let tx_pin = peripherals.GPIO4;
    let rx_pin = peripherals.GPIO5;
    let config = Config::default().with_baudrate(9600);
    let uart = Uart::new(peripherals.UART1, config)
        .expect("UART initialization failed")
        .with_rx(rx_pin)
        .with_tx(tx_pin)
        .into_async();

    // Configure I2C for OLED
    let i2c_bus = esp_hal::i2c::master::I2c::new(
        peripherals.I2C0,
        esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_scl(peripherals.GPIO9)
    .with_sda(peripherals.GPIO6)
    .into_async();

    let interface = I2CDisplayInterface::new(i2c_bus);
    let mut display = Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().await.unwrap();

    // Spawn tasks
    spawner.spawn(gps_task(uart)).unwrap();
    spawner.spawn(display_task(display)).unwrap();

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn gps_task(mut uart: Uart<'static, Async>) {
    let mut sentence = [0u8; 128];
    let mut idx = 0;

    loop {
        let mut buffer = [0u8; 1];
        match uart.read_async(&mut buffer).await {
            Ok(_) => {
                let byte = buffer[0];
                if idx < sentence.len() {
                    sentence[idx] = byte;
                    idx += 1;
                }

                if byte == b'\n' || byte == b'\r' || idx == sentence.len() {
                    if idx > 1 {
                        if let Ok(s) = core::str::from_utf8(&sentence[..idx]) {
                            let sentence_str = s.trim();
                            if sentence_str.starts_with("$GPGGA") {
                                if let Some((lat, lon, sats)) = parse_gga(sentence_str) {
                                    defmt::info!("GPGGA: Lat {}, Lon {}, Sats {}", lat, lon, sats);
                                    let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
                                    let mut gps_data = gps_ref.lock().await;
                                    gps_data.lat = lat;
                                    gps_data.lon = lon;
                                    gps_data.satellites = sats;
                                    gps_data.valid = true;
                                    // Note: speed and heading are preserved from previous GPRMC
                                }
                            } else if sentence_str.starts_with("$GPRMC") {
                                if let Some((lat, lon, speed, heading)) = parse_rmc(sentence_str) {
                                    defmt::info!(
                                        "GPRMC: Lat {}, Lon {}, Speed {} knots, Heading {}°",
                                        lat,
                                        lon,
                                        speed,
                                        heading
                                    );
                                    let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
                                    let mut gps_data = gps_ref.lock().await;
                                    gps_data.lat = lat;
                                    gps_data.lon = lon;
                                    gps_data.speed = speed;
                                    gps_data.heading = heading;
                                    gps_data.valid = true;
                                    // Note: satellites count is preserved from previous GPGGA
                                }
                            }
                        }
                    }
                    idx = 0;
                }
            }
            Err(e) => {
                defmt::error!("UART error: {:?}", e);
            }
        }
    }
}

#[embassy_executor::task]
async fn display_task(
    mut display: Ssd1306Async<
        ssd1306::prelude::I2CInterface<esp_hal::i2c::master::I2c<'static, esp_hal::Async>>,
        DisplaySize128x64,
        ssd1306::mode::BufferedGraphicsModeAsync<DisplaySize128x64>,
    >,
) {
    loop {
        display.clear(BinaryColor::Off).unwrap();

        let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
        let gps_data = gps_ref.lock().await;
        let data_copy = *gps_data;
        drop(gps_data);

        draw_gps_ui(&mut display, &data_copy).unwrap();
        display.flush().await.unwrap();

        Timer::after(Duration::from_secs(1)).await;
    }
}

fn draw_gps_ui<D>(display: &mut D, gps: &GpsData) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let text_style = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);

    if gps.valid {
        // Line 1: Latitude - max 14 chars: "Lat:12.34567"
        let mut lat_str: String<16> = String::new();
        write!(lat_str, "Lat:{:.5}", gps.lat).unwrap();
        Text::with_baseline(&lat_str, Point::new(0, 0), text_style, Baseline::Top).draw(display)?;

        // Line 2: Longitude - max 14 chars: "Lon:123.45678"
        let mut lon_str: String<16> = String::new();
        write!(lon_str, "Lon:{:.5}", gps.lon).unwrap();
        Text::with_baseline(&lon_str, Point::new(0, 16), text_style, Baseline::Top)
            .draw(display)?;

        // Line 3: Satellites and Speed - max 14 chars: "Sat:12 Spd:3.4"
        let mut sat_speed_str: String<16> = String::new();
        write!(sat_speed_str, "Sat:{} Spd:{:.1}", gps.satellites, gps.speed).unwrap();
        Text::with_baseline(&sat_speed_str, Point::new(0, 32), text_style, Baseline::Top)
            .draw(display)?;

        // Line 4: Heading - max 14 chars: "Hd:123.4d"
        let mut heading_str: String<16> = String::new();
        write!(heading_str, "Hd:{:.1}d", gps.heading).unwrap();
        Text::with_baseline(&heading_str, Point::new(0, 48), text_style, Baseline::Top)
            .draw(display)?;
    } else {
        Text::with_baseline(
            "Waiting for GPS",
            Point::new(0, 16),
            text_style,
            Baseline::Top,
        )
        .draw(display)?;
        Text::with_baseline("fix...", Point::new(0, 32), text_style, Baseline::Top)
            .draw(display)?;
    }

    Ok(())
}

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
