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

/// Two connections in flight: the browser holds one for the upload while the
/// other stays free for the status polls the page makes alongside it.
pub const WEB_TASK_POOL_SIZE: usize = 2;

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    id: usize,
    stack: Stack<'static>,
    router: &'static AppRouter<Application>,
    config: &'static picoserve::Config<Duration>,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 2048];
    let mut tcp_tx_buffer = [0; 2048];
    let mut http_buffer = [0; 16384];

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

    defmt::info!(
        "OTA request: offset={}, len={}, final={}",
        params.offset,
        data.len(),
        params.is_final
    );

    // The first chunk opens the OTA session; later chunks just append.
    if params.offset == 0 {
        crate::ota::OTA_CHANNEL
            .send(crate::ota::OtaCommand::Start {
                size: params.size,
                crc: params.crc,
            })
            .await;

        // Give the OTA task a chance to open the partition before the first write
        // lands on the channel.
        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }

    // Fill the shared buffer before signalling, so the OTA task never reads a
    // chunk that has not been written yet. Scoped so the guard is released before
    // the send below parks this task.
    let len = data.len();
    {
        let mut buffer = crate::ota::OTA_BUFFER.lock().await;
        buffer[..len].copy_from_slice(data);
    }

    // Send write command
    crate::ota::OTA_CHANNEL
        .send(crate::ota::OtaCommand::WriteChunk { len })
        .await;

    if params.is_final {
        crate::ota::OTA_CHANNEL
            .send(crate::ota::OtaCommand::Finish)
            .await;
    }

    // Let the WiFi TX queue drain before replying; without this the response can
    // be dropped under a sustained upload and the browser retries the chunk.
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    picoserve::response::Json(OtaResponse {
        success: true,
        message: if params.is_final { "done" } else { "ok" },
    })
}
