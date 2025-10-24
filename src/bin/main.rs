#![no_std]
#![no_main]

extern crate alloc;

// We bring the allocator into scope here.
use esp_alloc as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output};
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use esp_hal_smartled::SmartLedsAdapter;
use esp_println as _;

// Import GPS module
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
    // Correct location for the heap allocator macro: inside main.
    esp_alloc::heap_allocator!(size: 32 * 1024);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    let gps_data_mutex = embassy_sync::mutex::Mutex::new(GpsData::default());
    let gps_data_ref = GPS_DATA_CELL.init(gps_data_mutex);
    unsafe {
        GPS_DATA_REF = Some(gps_data_ref);
    }

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
