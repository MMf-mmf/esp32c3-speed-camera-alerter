#![no_std]
#![no_main]

extern crate alloc;

use esp_alloc as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, Pull};
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
    esp_println::println!("{}", info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_alloc::heap_allocator!(size: 180 * 1024);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    // Initialize system in GPS mode
    gps::mode::set_mode(gps::mode::SystemMode::GpsMode);
    esp_println::println!("System starting in GPS mode");

    // Initialize GPS data mutex
    let gps_data_mutex = embassy_sync::mutex::Mutex::new(GpsData::default());
    let gps_data_ref = GPS_DATA_CELL.init(gps_data_mutex);
    unsafe {
        GPS_DATA_REF = Some(gps_data_ref);
    }

    // Initialize button on GPIO9 (with pull-up for active-low)
    let button = Input::new(
        peripherals.GPIO9,
        InputConfig::default().with_pull(Pull::Up),
    );

    spawner.spawn(gps::button::button_task(button)).unwrap();

    // Initialize GPS UART
    let tx_pin = peripherals.GPIO4;
    let rx_pin = peripherals.GPIO5;
    let uart_config = Config::default().with_baudrate(9600);
    let uart = Uart::new(peripherals.UART1, uart_config)
        .expect("UART initialization failed")
        .with_rx(rx_pin)
        .with_tx(tx_pin)
        .into_async();

    // Initialize RMT for LED control
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let rmt_buffer = [0u32; 25];
    let led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO8, rmt_buffer);

    // Initialize buzzer on GPIO2
    let buzzer = Output::new(peripherals.GPIO2, Level::Low, Default::default());

    // Spawn GPS tasks (will check mode internally)
    spawner.spawn(gps_task(uart)).unwrap();
    spawner.spawn(proximity_check_task()).unwrap();
    spawner.spawn(led_control_task(led)).unwrap();
    spawner.spawn(buzzer_control_task(buzzer)).unwrap();

    // Initialize WiFi controller (but don't start it yet)
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let esp_wifi_ctrl = &*gps::mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timer1.timer0, rng.clone()).unwrap()
    );

    // Spawn WiFi mode manager task
    spawner
        .spawn(wifi_mode_manager_task(esp_wifi_ctrl, peripherals.WIFI, rng))
        .unwrap();

    esp_println::println!("System initialized - Long press button to toggle WiFi AP mode");

    // Main loop - monitor mode changes
    loop {
        let mode = gps::mode::MODE_CHANGE_SIGNAL.wait().await;
        match mode {
            gps::mode::SystemMode::GpsMode => {
                esp_println::println!("Main: System in GPS mode");
            }
            gps::mode::SystemMode::WifiApMode => {
                esp_println::println!("Main: System in WiFi AP mode - OTA updates available");
            }
        }
    }
}

#[embassy_executor::task]
async fn wifi_mode_manager_task(
    esp_wifi_ctrl: &'static EspWifiController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    rng: Rng,
) {
    // Wait for first WiFi mode request
    while !gps::mode::is_wifi_mode() {
        Timer::after(Duration::from_millis(100)).await;
    }

    esp_println::println!("WiFi Manager: Starting WiFi AP...");

    // Get the spawner for this task
    let spawner = embassy_executor::Spawner::for_current_executor().await;

    match gps::wifi::start_wifi(esp_wifi_ctrl, wifi, rng, &spawner).await {
        Ok(_stack) => {
            esp_println::println!("WiFi AP started successfully!");

            // Wait for shutdown request
            while !gps::mode::is_wifi_shutdown_requested() {
                Timer::after(Duration::from_millis(500)).await;
            }

            esp_println::println!("WiFi Manager: Shutdown requested, cleaning up...");
            esp_hal_dhcp_server::dhcp_close();
            Timer::after(Duration::from_secs(1)).await;
            esp_println::println!("WiFi Manager: Stopped, returning to GPS mode");
        }
        Err(e) => {
            esp_println::println!("Failed to start WiFi: {:?}", e);
            gps::mode::request_gps_mode();
        }
    }
}
