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
use esp_wifi::wifi::{self, WifiController, WifiDevice, WifiEvent, WifiState};
use esp_wifi::EspWifiController;

use crate::mk_static;

const SSID: &str = "SpeedMe";
const PASSWORD: &str = "12345678";
const STATIC_IP: &str = "192.168.13.37/24";
const GATEWAY_IP: &str = "192.168.13.37";

pub async fn start_wifi(
    esp_wifi_ctrl: &'static EspWifiController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    rng: Rng,
    spawner: &Spawner,
) -> anyhow::Result<Stack<'static>> {
    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, wifi).unwrap();
    let wifi_interface = interfaces.ap;
    let mut rng = rng;
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    let ip_addr =
        Ipv4Cidr::from_str(STATIC_IP).map_err(|_| anyhow!("Invalid STATIC_IP: {}", STATIC_IP))?;
    let gateway = Ipv4Addr::from_str(GATEWAY_IP)
        .map_err(|_| anyhow!("Invalid GATEWAY_IP: {}", GATEWAY_IP))?;

    let net_config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: ip_addr,
        gateway: Some(gateway),
        dns_servers: Default::default(),
    });

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        net_seed,
    );

    spawner.spawn(connection_task(controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    wait_for_connection(stack).await;

    spawner.spawn(dhcp_server_task(stack)).ok();

    Ok(stack)
}

async fn wait_for_connection(stack: Stack<'_>) {
    defmt::info!("Waiting for link to be up");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    defmt::info!("Connect to AP `{}` with password `{}`", SSID, PASSWORD);
    defmt::info!("Then browse to http://{}/", GATEWAY_IP);

    while !stack.is_config_up() {
        Timer::after(Duration::from_millis(100)).await
    }

    stack
        .config_v4()
        .inspect(|c| defmt::info!("IPv4 config: {:?}", c));
}

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    defmt::info!("WiFi connection task started");
    defmt::info!("Device capabilities: {:?}", controller.capabilities());

    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::ApStarted => {
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
            defmt::info!("Starting WiFi AP");
            controller.start_async().await.unwrap();
            defmt::info!("WiFi AP started!");
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn dhcp_server_task(stack: Stack<'static>) {
    defmt::info!("Starting DHCP server...");

    let config = DhcpServerConfig {
        ip: DhcpIpv4Addr::new(192, 168, 13, 37),
        lease_time: Duration::from_secs(3600),
        gateways: &[DhcpIpv4Addr::new(192, 168, 13, 37)],
        subnet: None,
        dns: &[DhcpIpv4Addr::new(192, 168, 13, 37)],
        use_captive_portal: false,
    };

    let mut leaser = SimpleDhcpLeaser {
        start: DhcpIpv4Addr::new(192, 168, 13, 50),
        end: DhcpIpv4Addr::new(192, 168, 13, 200),
        leases: Default::default(),
    };

    defmt::info!("DHCP server: Assigning IPs from 192.168.13.50 to 192.168.13.200");

    let res = esp_hal_dhcp_server::run_dhcp_server(stack, config, &mut leaser).await;
    if let Err(e) = res {
        defmt::error!("DHCP SERVER ERROR: {:?}", e);
    }

    defmt::info!("DHCP server task ended");
}
