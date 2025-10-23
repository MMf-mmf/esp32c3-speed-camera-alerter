#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::rmt::Rmt;
use esp_hal::rng::Rng;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal_smartled::SmartLedsAdapter;
use esp_println as _;
use smart_leds::RGB8;
use smart_leds::{brightness, SmartLedsWrite};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

use esp_wifi::EspWifiController;
use gps::mk_static;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // ESP32-C3 heap allocation: 150KB
    // This balances heap needs with DRAM constraints (~328KB usable DRAM total)
    // Leaves sufficient space for stack, WiFi buffers, and static variables
    esp_alloc::heap_allocator!(size: 150 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    info!("Embassy initialized!");

    // Initialize LED to show WiFi mode (solid blue)
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let rmt_buffer = [0u32; 25];
    let mut led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO8, rmt_buffer);

    let color_blue = RGB8 { r: 0, g: 0, b: 255 };
    led.write(brightness(core::iter::once(color_blue), 50)).ok();

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timer1.timer0, rng.clone()).unwrap()
    );

    let stack = gps::wifi::start_wifi(esp_wifi_ctrl, peripherals.WIFI, rng, &spawner)
        .await
        .unwrap();

    // Add a small delay to ensure stack is fully ready
    Timer::after(embassy_time::Duration::from_millis(1000)).await;

    // Initialize OTA and mark current app as valid
    if let Err(_) = gps::ota::ota_init() {
        info!("OTA init failed (expected if running from factory partition)");
    }

    // Spawn OTA task
    spawner.must_spawn(gps::ota::ota_task());

    let web_app = gps::web::WebApp::default();
    for id in 0..gps::web::WEB_TASK_POOL_SIZE {
        spawner.must_spawn(gps::web::web_task(
            id,
            stack,
            web_app.router,
            web_app.config,
        ));
    }
    info!("Web server started...");
    info!("WiFi Mode Active - Connect to 'SpeedMe' and visit http://192.168.13.37");

    loop {
        Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
