#![no_std]
#![no_main]

extern crate alloc;

use esp_alloc as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::peripherals::Peripherals;
use esp_hal::rmt::Rmt;
use esp_hal::rng::Rng;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use esp_hal_smartled::SmartLedsAdapter;
use esp_println as _;
use esp_wifi::EspWifiController;

use gps::gps::{buzzer_control_task, gps_task, led_control_task, proximity_check_task};
use gps::gps::{GpsData, GPS_DATA_CELL, GPS_DATA_REF};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    defmt::error!("PANIC: {}", defmt::Display2Format(info));
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 170 * 1024);
    // Check RTC memory to determine boot mode
    let boot_mode = gps::mode::get_boot_mode();

    match boot_mode {
        gps::mode::BootMode::GpsMode => {
            defmt::info!("=== BOOTING INTO GPS MODE ===");
            init_gps_mode(spawner, peripherals).await;
        }
        gps::mode::BootMode::WifiMode => {
            defmt::info!("=== BOOTING INTO WIFI MODE ===");
            init_wifi_mode(spawner, peripherals).await;
        }
    }
}

async fn init_gps_mode(spawner: Spawner, peripherals: Peripherals) -> ! {
    // Initialize embassy timer
    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    // Initialize GPS data mutex
    let gps_data_mutex = embassy_sync::mutex::Mutex::new(GpsData::default());
    let gps_data_ref = GPS_DATA_CELL.init(gps_data_mutex);
    unsafe {
        GPS_DATA_REF = Some(gps_data_ref);
    }

    // TODO: Initialize button on GPIO10 (with pull-up for active-low)
    let button = Input::new(
        peripherals.GPIO10, // 9 for normal
        InputConfig::default().with_pull(Pull::Up),
    );

    spawner
        .spawn(gps::button::button_task(button, false))
        .unwrap();

    // Initialize GPS UART
    let tx_pin = peripherals.GPIO20; // 4 for normal
    let rx_pin = peripherals.GPIO21; // 5 for normal
    let uart_config = Config::default().with_baudrate(9600);
    let uart = Uart::new(peripherals.UART1, uart_config)
        .expect("UART initialization failed")
        .with_rx(rx_pin)
        .with_tx(tx_pin)
        .into_async();

    // Initialize RMT for LED control
    let rmt: Rmt<'_, esp_hal::Blocking> =
        Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let rmt_buffer = [0u32; 75];
    let led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO8, rmt_buffer);
    // let led = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());

    // Initialize buzzer on GPIO2
    let buzzer = Output::new(peripherals.GPIO2, Level::Low, Default::default());

    // Spawn GPS tasks
    spawner.spawn(gps_task(uart)).unwrap();
    spawner.spawn(proximity_check_task()).unwrap();
    spawner.spawn(led_control_task(led)).unwrap();
    spawner.spawn(buzzer_control_task(buzzer)).unwrap();

    defmt::info!("GPS mode initialized - Long press button to switch to WiFi mode");

    // Keep main task alive
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

async fn init_wifi_mode(spawner: Spawner, peripherals: Peripherals) -> ! {
    // Initialize embassy timer
    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    // Clear the RTC boot mode so next reboot goes to GPS mode
    gps::mode::clear_boot_mode();

    // Initialize button on GPIO9 (with pull-up for active-low)
    let button = Input::new(
        peripherals.GPIO9,
        InputConfig::default().with_pull(Pull::Up),
    );

    spawner
        .spawn(gps::button::button_task(button, true))
        .unwrap();

    // Initialize WiFi controller
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let esp_wifi_ctrl = &*gps::mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timer1.timer0, rng.clone()).unwrap()
    );

    defmt::info!("Starting WiFi AP...");

    let stack = match gps::wifi::start_wifi(esp_wifi_ctrl, peripherals.WIFI, rng, &spawner).await {
        Ok(s) => s,
        Err(e) => {
            defmt::error!("Failed to start WiFi: {}", defmt::Debug2Format(&e));
            defmt::info!("Rebooting to GPS mode in 3 seconds...");
            Timer::after(Duration::from_secs(3)).await;
            esp_hal::system::software_reset();
        }
    };

    // FIXED: Add delay to ensure stack is fully ready (matching working example)
    Timer::after(Duration::from_millis(1000)).await;

    // Initialize OTA and mark current app as valid
    if let Err(_) = gps::ota::ota_init() {
        defmt::warn!("OTA init failed (expected if running from factory partition)");
    }

    // Spawn OTA task
    spawner.must_spawn(gps::ota::ota_task());

    // FIXED: Spawn web server tasks with proper loop for pool size
    let web_app = gps::web::WebApp::default();
    for id in 0..gps::web::WEB_TASK_POOL_SIZE {
        spawner.must_spawn(gps::web::web_task(
            id,
            stack,
            web_app.router,
            web_app.config,
        ));
    }
    defmt::info!("Web server with OTA started on http://192.168.13.37/");
    defmt::info!("Long press button to return to GPS mode");

    // Keep main task alive
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
