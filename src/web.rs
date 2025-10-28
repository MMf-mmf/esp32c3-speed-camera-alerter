use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc as _;
use picoserve::response::IntoResponse;
use picoserve::{response::File, routing, AppBuilder, AppRouter, Router};

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route(
                "/",
                routing::get_service(File::html(include_str!("index.html"))),
            )
            .route("/ota", routing::post(ota_handler))
    }
}

pub const WEB_TASK_POOL_SIZE: usize = 1;

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    id: usize,
    stack: Stack<'static>,
    router: &'static AppRouter<Application>,
    config: &'static picoserve::Config<Duration>,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 512];
    let mut tcp_tx_buffer = [0; 512];
    let mut http_buffer = [0; 4096];

    picoserve::listen_and_serve(
        id,
        router,
        config,
        stack,
        port,
        &mut tcp_rx_buffer,
        &mut tcp_tx_buffer,
        &mut http_buffer,
    )
    .await
}

pub struct WebApp {
    pub router: &'static Router<<Application as AppBuilder>::PathRouter>,
    pub config: &'static picoserve::Config<Duration>,
}

impl Default for WebApp {
    fn default() -> Self {
        let router = picoserve::make_static!(AppRouter<Application>, Application.build_app());

        let config = picoserve::make_static!(
            picoserve::Config<Duration>,
            picoserve::Config::new(picoserve::Timeouts {
                start_read_request: Some(Duration::from_secs(10)),
                read_request: Some(Duration::from_secs(5)),
                write: Some(Duration::from_secs(5)),
            })
            .keep_connection_alive()
        );

        Self { router, config }
    }
}

#[derive(serde::Serialize)]
struct OtaResponse {
    success: bool,
    message: &'static str,
}

#[derive(serde::Deserialize)]
struct OtaQueryParams {
    size: u32,
    crc: u32,
    offset: u32,
    is_final: bool,
}

struct BinaryBody {
    data: heapless::Vec<u8, 2048>,
}

impl<'r, State> picoserve::extract::FromRequest<'r, State> for BinaryBody {
    type Rejection = core::convert::Infallible;

    async fn from_request<R: picoserve::io::Read>(
        _state: &'r State,
        _request_parts: picoserve::request::RequestParts<'r>,
        request_body: picoserve::request::RequestBody<'r, R>,
    ) -> Result<Self, Self::Rejection> {
        let mut data = heapless::Vec::new();
        if let Ok(body_bytes) = request_body.read_all().await {
            let _ = data.extend_from_slice(body_bytes);
        }
        Ok(BinaryBody { data })
    }
}

async fn ota_handler(
    query: picoserve::extract::Query<OtaQueryParams>,
    body: BinaryBody,
) -> impl IntoResponse {
    let params = query.0;
    let data = &body.data;

    esp_println::println!(
        "OTA request: offset={}, len={}, final={}",
        params.offset,
        data.len(),
        params.is_final
    );

    // Send start command only for first chunk (offset 0)
    if params.offset == 0 {
        crate::ota::OTA_CHANNEL
            .send(crate::ota::OtaCommand::Start {
                size: params.size,
                crc: params.crc,
            })
            .await;

        // Small delay to let OTA task initialize
        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }

    // Copy data to OTA buffer
    let buffer_guard = crate::ota::OTA_BUFFER.lock().await;
    let mut buffer = buffer_guard.borrow_mut();
    let len = data.len();
    buffer[..len].copy_from_slice(data);
    drop(buffer);
    drop(buffer_guard);

    // Send write command
    crate::ota::OTA_CHANNEL
        .send(crate::ota::OtaCommand::WriteChunk { len })
        .await;

    // Send finish command only for last chunk
    if params.is_final {
        crate::ota::OTA_CHANNEL
            .send(crate::ota::OtaCommand::Finish)
            .await;
    }

    // Delay before sending response to let WiFi TX queue clear
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    picoserve::response::Json(OtaResponse {
        success: true,
        message: if params.is_final { "done" } else { "ok" },
    })
}
