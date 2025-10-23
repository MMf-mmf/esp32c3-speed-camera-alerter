// Embassy Access Point module with DHCP server
// https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.0.0-beta.0/examples/src/bin/wifi_embassy_access_point.rs
//! - creates an access-point with SSID `SpeedMe` and password `12345678`
//! - automatically assigns IP addresses to connected clients via DHCP
//! - you can connect to it and your device will automatically receive an IP address
//! - open http://192.168.13.37/ in your browser

use core::net::Ipv4Addr;
use core::str::FromStr;

use anyhow::anyhow;
use embassy_executor::Spawner;
use embassy_net::{Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
use esp_hal_dhcp_server::{
    simple_leaser::SimpleDhcpLeaser, structs::DhcpServerConfig, Ipv4Addr as DhcpIpv4Addr,
};
use esp_println as _;
use esp_println::println;
use esp_wifi::wifi::{self, WifiController, WifiDevice, WifiEvent, WifiState};
use esp_wifi::EspWifiController;

use crate::mk_static;

const SSID: &str = "SpeedMe";
const PASSWORD: &str = "12345678";

// Unlike Station mode, You can give any IP range(private) that you like
// IP Address/Subnet mask eg: STATIC_IP=192.168.13.37/24
const STATIC_IP: &str = "192.168.13.37/24";
// Gateway IP eg: GATEWAY_IP="192.168.13.37"
const GATEWAY_IP: &str = "192.168.13.37";

pub async fn start_wifi(
    esp_wifi_ctrl: &'static EspWifiController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    mut rng: Rng,
    spawner: &Spawner,
) -> anyhow::Result<Stack<'static>> {
    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, wifi).unwrap();
    let wifi_interface = interfaces.ap;
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    // Parse STATIC_IP
    let ip_addr =
        Ipv4Cidr::from_str(STATIC_IP).map_err(|_| anyhow!("Invalid STATIC_IP: {}", STATIC_IP))?;

    // Parse GATEWAY_IP
    let gateway = Ipv4Addr::from_str(GATEWAY_IP)
        .map_err(|_| anyhow!("Invalid GATEWAY_IP: {}", GATEWAY_IP))?;

    // Create Network config with IP details
    let net_config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: ip_addr,
        gateway: Some(gateway),
        dns_servers: Default::default(),
    });

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        net_seed,
    );

    spawner.spawn(connection_task(controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    wait_for_connection(stack).await;

    // Start DHCP server
    spawner.spawn(dhcp_server_task(stack)).ok();

    Ok(stack)
}

async fn wait_for_connection(stack: Stack<'_>) {
    println!("Waiting for link to be up");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Connect to the AP `{}` with password `{}` and your device will automatically receive an IP address", SSID, PASSWORD);
    println!("Then point your browser to http://{}/", GATEWAY_IP);
    while !stack.is_config_up() {
        Timer::after(Duration::from_millis(100)).await
    }
    stack
        .config_v4()
        .inspect(|c| println!("ipv4 config: {c:?}"));
}

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::ApStarted => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::ApStop).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = wifi::Configuration::AccessPoint(wifi::AccessPointConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                auth_method: esp_wifi::wifi::AuthMethod::WPA2Personal,
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn dhcp_server_task(stack: Stack<'static>) {
    println!("Starting DHCP server...");

    let config = DhcpServerConfig {
        ip: DhcpIpv4Addr::new(192, 168, 13, 37),
        lease_time: Duration::from_secs(3600), // 1 hour lease time
        gateways: &[DhcpIpv4Addr::new(192, 168, 13, 37)],
        subnet: None,
        dns: &[DhcpIpv4Addr::new(192, 168, 13, 37)],
        use_captive_portal: false,
    };

    // DHCP will assign IPs from 192.168.13.50 to 192.168.13.200
    let mut leaser = SimpleDhcpLeaser {
        start: DhcpIpv4Addr::new(192, 168, 13, 50),
        end: DhcpIpv4Addr::new(192, 168, 13, 200),
        leases: Default::default(),
    };

    println!("DHCP server configured to assign IPs from 192.168.13.50 to 192.168.13.200");

    let res = esp_hal_dhcp_server::run_dhcp_server(stack, config, &mut leaser).await;
    if let Err(e) = res {
        println!("DHCP SERVER ERROR: {e:?}");
    }
}
