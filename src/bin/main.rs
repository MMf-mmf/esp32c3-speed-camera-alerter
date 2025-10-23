#![no_std]
#![no_main]

extern crate alloc;

// We bring the allocator into scope here.
use esp_alloc as _;

use core::f64::consts::PI;
use core::fmt::Write;
use defmt;
use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output};
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use esp_hal::Async;
use esp_hal_smartled::SmartLedsAdapter;
use esp_println as _;
use geohash::{encode, Coord, Direction};
use heapless::String as HString;
use libm::{atan2, cos, sin, sqrt};
use smart_leds::RGB8;
use smart_leds::{brightness, gamma, SmartLedsWrite};
use static_cell::StaticCell;

// Type alias for the LED adapter
type LedType = SmartLedsAdapter<esp_hal::rmt::ConstChannelAccess<esp_hal::rmt::Tx, 0>, 25>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

include!(concat!(env!("OUT_DIR"), "/geodata.rs"));

#[derive(Clone)]
struct GpsData {
    lat: f32,
    lon: f32,
    satellites: u8,
    speed: f32,
    heading: f32,
    valid: bool,
    notification: Option<HString<12>>,
    time_hours: u8,
    time_minutes: u8,
    time_seconds: u8,
    buzzer_triggered: bool,
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
            notification: None,
            time_hours: 0,
            time_minutes: 0,
            time_seconds: 0,
            buzzer_triggered: false,
        }
    }
}

static GPS_DATA_CELL: StaticCell<
    Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, GpsData>,
> = StaticCell::new();
static mut GPS_DATA_REF: Option<
    &'static Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, GpsData>,
> = None;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("{}", info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Correct location for the heap allocator macro: inside main.
    esp_alloc::heap_allocator!(size: 32 * 1024);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    let gps_data_mutex = Mutex::new(GpsData::default());
    let gps_data_ref = GPS_DATA_CELL.init(gps_data_mutex);
    unsafe {
        GPS_DATA_REF = Some(gps_data_ref);
    }

    // const TEST_HASHES: [&str; 3] = ["dp3ts4g", "dp3w6ry", "dp3wdbh"];
    // // loop over the list get the hash value and print it to the screen

    // for &hash in &TEST_HASHES {
    //     if let Some(coord) = GEO_MAP.get(hash) {
    //         defmt::info!(
    //             "Geohash: {}, Latitude: {}, Longitude: {}",
    //             hash,
    //             coord.latitude,
    //             coord.longitude
    //         );
    //     } else {
    //         defmt::warn!("Geohash {} not found in GEO_MAP", hash);
    //     }
    // }

    let tx_pin = peripherals.GPIO4;
    let rx_pin = peripherals.GPIO5;

    let uart_config = Config::default().with_baudrate(9600);

    // The UART driver now manages its own buffering internally
    let uart = Uart::new(peripherals.UART1, uart_config)
        .expect("UART initialization failed")
        .with_rx(rx_pin)
        .with_tx(tx_pin)
        .into_async();

    // Initialize RMT for LED control
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let rmt_buffer = [0u32; 25]; // (1 LED * 24 bits) + 1
    let led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO8, rmt_buffer);

    // Initialize buzzer on GPIO2
    let buzzer = Output::new(peripherals.GPIO2, Level::Low, Default::default());

    spawner.spawn(gps_task(uart)).unwrap();
    spawner.spawn(proximity_check_task()).unwrap();
    spawner.spawn(led_control_task(led)).unwrap();
    spawner.spawn(buzzer_control_task(buzzer)).unwrap();

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn led_control_task(mut led: LedType) {
    const BRIGHTNESS_LOW: u8 = 10;
    const BRIGHTNESS_HIGH: u8 = 100;
    const GREEN_BLINK_INTERVAL_MS: u64 = 10000; // 10 seconds between blinks
    const GREEN_BLINK_DURATION_MS: u64 = 200; // 200ms blink duration

    let color_red = RGB8 { r: 255, g: 0, b: 0 };
    let color_green = RGB8 { r: 0, g: 255, b: 0 };
    let color_yellow = RGB8 {
        r: 255,
        g: 255,
        b: 0,
    };
    let color_off = RGB8 { r: 0, g: 0, b: 0 };

    let mut blink_state = false;
    let mut green_timer_ms: u64 = 0;

    loop {
        let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
        let gps_data = gps_ref.lock().await;

        if gps_data.notification.is_some() {
            // State 1: Heading to camera - RED at high brightness (continuous)
            led.write(brightness(
                gamma(core::iter::once(color_red)),
                BRIGHTNESS_HIGH,
            ))
            .ok();
            drop(gps_data);
            green_timer_ms = 0; // Reset green timer when not in green state
            Timer::after(Duration::from_millis(500)).await;
        } else if gps_data.valid {
            // State 2: GPS fix but not heading to camera - GREEN blink every 10 seconds
            drop(gps_data);

            if green_timer_ms >= GREEN_BLINK_INTERVAL_MS {
                // Time for a green blink
                led.write(brightness(
                    gamma(core::iter::once(color_green)),
                    BRIGHTNESS_LOW,
                ))
                .ok();
                Timer::after(Duration::from_millis(GREEN_BLINK_DURATION_MS)).await;

                // Turn off LED after blink
                led.write(core::iter::once(color_off)).ok();
                green_timer_ms = 0; // Reset timer
            } else {
                // LED stays off, just increment timer
                led.write(core::iter::once(color_off)).ok();
            }

            // Wait 1 second and increment timer (reduces mutex contention with GPS task)
            Timer::after(Duration::from_secs(1)).await;
            green_timer_ms += 1000;
        } else {
            // State 3: No GPS fix - YELLOW blinking at low brightness (1s on/1s off)
            drop(gps_data);
            green_timer_ms = 0; // Reset green timer when not in green state

            if blink_state {
                led.write(brightness(
                    gamma(core::iter::once(color_yellow)),
                    BRIGHTNESS_LOW,
                ))
                .ok();
            } else {
                led.write(core::iter::once(color_off)).ok();
            }

            blink_state = !blink_state;
            Timer::after(Duration::from_secs(1)).await;
        }
    }
}

#[embassy_executor::task]
async fn buzzer_control_task(mut buzzer: Output<'static>) {
    const BUZZ_DURATION_MS: u64 = 100; // 100ms per buzz
    const BUZZ_GAP_MS: u64 = 100; // 100ms gap between buzzes

    loop {
        Timer::after(Duration::from_millis(100)).await;

        let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
        let mut gps_data = gps_ref.lock().await;

        if gps_data.notification.is_some() && !gps_data.buzzer_triggered {
            // Camera detected and we haven't buzzed yet
            defmt::info!("🔊 Camera detected - triggering buzzer!");

            // Mark as triggered before buzzing to prevent re-entry
            gps_data.buzzer_triggered = true;
            drop(gps_data);

            // First buzz
            buzzer.set_high();
            Timer::after(Duration::from_millis(BUZZ_DURATION_MS)).await;
            buzzer.set_low();

            // Gap between buzzes
            Timer::after(Duration::from_millis(BUZZ_GAP_MS)).await;

            // Second buzz
            buzzer.set_high();
            Timer::after(Duration::from_millis(BUZZ_DURATION_MS)).await;
            buzzer.set_low();

            defmt::info!("🔊 Buzzer sequence complete");
        } else if gps_data.notification.is_none() && gps_data.buzzer_triggered {
            // No camera detected anymore, reset the flag
            gps_data.buzzer_triggered = false;
            drop(gps_data);
        } else {
            drop(gps_data);
        }
    }
}

#[embassy_executor::task]
async fn proximity_check_task() {
    const PRECISION: usize = 7;
    const DISTANCE_THRESHOLD_KM: f64 = 0.244; // 800 feet
    const HEADING_TOLERANCE_DEG: f64 = 25.0;
    const MINIMUM_SPEED_KNOTS: f32 = 7.0; // 8 mph
    loop {
        Timer::after(Duration::from_secs(3)).await;

        let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
        let mut gps_data = gps_ref.lock().await;

        if gps_data.valid && gps_data.speed > MINIMUM_SPEED_KNOTS {
            let current_pos = Coord {
                y: gps_data.lat as f64,
                x: gps_data.lon as f64,
            };
            let central_hash_str = encode(current_pos, PRECISION).unwrap();

            let mut search_hashes: heapless::Vec<HString<12>, 9> = heapless::Vec::new();
            let mut central_key = HString::<12>::new();
            write!(central_key, "{}", &central_hash_str).unwrap();
            search_hashes.push(central_key).ok();

            for dir in [
                Direction::N,
                Direction::NE,
                Direction::E,
                Direction::SE,
                Direction::S,
                Direction::SW,
                Direction::W,
                Direction::NW,
            ] {
                if let Ok(neighbor) = geohash::neighbor(&central_hash_str, dir) {
                    let mut key = HString::<12>::new();
                    write!(key, "{}", neighbor).unwrap();
                    search_hashes.push(key).ok();
                }
            }

            let mut closest_location: Option<HString<12>> = None;
            let mut closest_dist = f64::MAX;

            for hash_key in search_hashes.iter() {
                if let Some(candidate_coord) = GEO_MAP.get(hash_key.as_str()) {
                    defmt::info!("Candidate location geohash: {}", hash_key.as_str());

                    let distance = calculate_distance(
                        current_pos.y,
                        current_pos.x,
                        candidate_coord.latitude,
                        candidate_coord.longitude,
                    );

                    if distance < DISTANCE_THRESHOLD_KM {
                        let required_heading = calculate_heading(
                            current_pos.y,
                            current_pos.x,
                            candidate_coord.latitude,
                            candidate_coord.longitude,
                        );
                        let heading_diff = (gps_data.heading as f64 - required_heading).abs();

                        if heading_diff <= HEADING_TOLERANCE_DEG
                            || (360.0 - heading_diff) <= HEADING_TOLERANCE_DEG
                        {
                            defmt::info!("✅ Heading towards geohash {}!", hash_key.as_str());
                            if distance < closest_dist {
                                closest_dist = distance;
                                // Since we don't have names, we'll use the geohash for notification
                                closest_location = Some(hash_key.clone());
                            }
                        }
                    }
                }
            }
            gps_data.notification = closest_location;
        } else if gps_data.valid {
            gps_data.notification = None;
        }
    }
}

#[embassy_executor::task]
async fn gps_task(mut uart: Uart<'static, Async>) {
    // A buffer to build the current NMEA sentence
    let mut sentence = [0u8; 128];
    let mut idx = 0;

    // A larger buffer to read chunks of data from the UART efficiently
    let mut read_buf = [0u8; 64];

    loop {
        // Wait for data and read as much as is available (up to 64 bytes)
        match uart.read_async(&mut read_buf).await {
            Ok(bytes_read) => {
                // Process each byte that we just read in the chunk
                for &byte in &read_buf[..bytes_read] {
                    // Add the byte to our sentence buffer if there's space
                    if idx < sentence.len() {
                        sentence[idx] = byte;
                        idx += 1;
                    }

                    // Check if we've reached the end of a line (or the buffer is full)
                    if byte == b'\n' || byte == b'\r' || idx == sentence.len() {
                        // Only process if the sentence has content
                        if idx > 1 {
                            if let Ok(s) = core::str::from_utf8(&sentence[..idx]) {
                                let sentence_str = s.trim();
                                if sentence_str.starts_with("$GPGGA") {
                                    // GPGGA: Only update satellite count
                                    if let Some(sats) = parse_gga(sentence_str) {
                                        defmt::info!("GPGGA: Sats {}", sats);
                                        let gps_ref = unsafe { GPS_DATA_REF.unwrap() };
                                        let mut gps_data = gps_ref.lock().await;
                                        gps_data.satellites = sats;
                                    }
                                } else if sentence_str.starts_with("$GPRMC") {
                                    // GPRMC: Primary source for position, speed, heading, and time
                                    if let Some((
                                        lat,
                                        lon,
                                        speed,
                                        heading,
                                        hours,
                                        minutes,
                                        seconds,
                                    )) = parse_rmc(sentence_str)
                                    {
                                        defmt::info!(
                                            "GPRMC: {}:{:02}:{:02} UTC, Lat {}, Lon {}, Speed {} knots, Heading {}°",
                                            hours,
                                            minutes,
                                            seconds,
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
                                        gps_data.time_hours = hours;
                                        gps_data.time_minutes = minutes;
                                        gps_data.time_seconds = seconds;
                                        gps_data.valid = true;
                                    }
                                }
                            }
                        }
                        // Reset the index to start building the next sentence
                        idx = 0;
                    }
                }
            }
            Err(e) => {
                // With chunked reading, overflows are much less likely, but we still handle errors
                idx = 0;
                defmt::error!("UART error: {:?}", e);
            }
        }
    }
}

fn nmea_to_decimal(coord: &str, dir: &str) -> Option<f32> {
    if coord.len() < 4 {
        return None;
    }
    let (degrees, minutes) = if let Some(split) = coord.find('.') {
        let deg_len = split - 2;
        (&coord[..deg_len], &coord[deg_len..])
    } else {
        let deg_len = coord.len() - 2;
        (&coord[..deg_len], &coord[deg_len..])
    };
    let deg: f32 = degrees.parse().ok()?;
    let min: f32 = minutes.parse().ok()?;
    let mut val = deg + (min / 60.0);
    if dir == "S" || dir == "W" {
        val = -val;
    }
    Some(val)
}

fn parse_nmea_time(time_str: &str) -> Option<(u8, u8, u8)> {
    // NMEA time format: HHMMSS.sss or HHMMSS
    if time_str.len() < 6 {
        return None;
    }
    let hours: u8 = time_str[0..2].parse().ok()?;
    let minutes: u8 = time_str[2..4].parse().ok()?;
    let seconds: u8 = time_str[4..6].parse().ok()?;
    Some((hours, minutes, seconds))
}

fn parse_gga(sentence: &str) -> Option<u8> {
    let fields: heapless::Vec<&str, 16> = sentence.split(',').collect();
    if fields.len() < 8 || fields[6] == "0" || fields[6].is_empty() {
        return None;
    }
    let sats: u8 = fields[7].parse().ok()?;
    Some(sats)
}

fn parse_rmc(sentence: &str) -> Option<(f32, f32, f32, f32, u8, u8, u8)> {
    let fields: heapless::Vec<&str, 16> = sentence.split(',').collect();
    if fields.len() < 9 || fields[2] != "A" {
        defmt::warn!("No GPS fix in RMC sentence");
        return None;
    }

    // Parse time from field 1
    let (hours, minutes, seconds) = parse_nmea_time(fields[1]).unwrap_or((0, 0, 0));

    let lat = nmea_to_decimal(fields[3], fields[4])?;
    let lon = nmea_to_decimal(fields[5], fields[6])?;
    let speed: f32 = fields[7].parse().ok().unwrap_or(0.0);
    let heading: f32 = fields[8].parse().ok().unwrap_or(0.0);
    Some((lat, lon, speed, heading, hours, minutes, seconds))
}

#[inline]
fn deg_to_rad(deg: f64) -> f64 {
    deg * (PI / 180.0)
}
#[inline]
fn rad_to_deg(rad: f64) -> f64 {
    rad * (180.0 / PI)
}

pub fn calculate_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let (lat1_rad, lon1_rad, lat2_rad, lon2_rad) = (
        deg_to_rad(lat1),
        deg_to_rad(lon1),
        deg_to_rad(lat2),
        deg_to_rad(lon2),
    );
    let (dlon, dlat) = (lon2_rad - lon1_rad, lat2_rad - lat1_rad);
    let sin_dlat_half = sin(dlat / 2.0);
    let sin_dlon_half = sin(dlon / 2.0);
    let a = sin_dlat_half * sin_dlat_half
        + cos(lat1_rad) * cos(lat2_rad) * sin_dlon_half * sin_dlon_half;
    2.0 * atan2(sqrt(a), sqrt(1.0 - a)) * R
}

pub fn calculate_heading(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1_rad, lon1_rad, lat2_rad, lon2_rad) = (
        deg_to_rad(lat1),
        deg_to_rad(lon1),
        deg_to_rad(lat2),
        deg_to_rad(lon2),
    );
    let dlon = lon2_rad - lon1_rad;
    let y = sin(dlon) * cos(lat2_rad);
    let x = cos(lat1_rad) * sin(lat2_rad) - sin(lat1_rad) * cos(lat2_rad) * cos(dlon);
    (rad_to_deg(atan2(y, x)) + 360.0) % 360.0
}
