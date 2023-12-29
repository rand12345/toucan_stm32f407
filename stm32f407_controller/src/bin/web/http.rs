use crate::errors::StmError;
use crate::statics::{BMS, CONFIG};
// use crate::types::messagebus::RequestType;
use crate::types::EthDevice;
use crate::utils::ByteMutWriter;
use bms_standard::BmsSerialise;
use defmt::{debug, error, info, unwrap, Debug2Format, Format};
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
// use embassy_net::IpListenEndpoint;
use embassy_net::{IpListenEndpoint, Stack};
// use embassy_stm32::usb_otg::In;
use embassy_time::{Duration, Timer};
use miniserde::{json, Deserialize, Serialize};

// use smoltcp::wire::IpListenEndpoint;
use url_lite::Url;

const INDEX_HTML: &[u8] = include_bytes!("html/index.html");
const SETTINGS_HTML: &[u8] = include_bytes!("html/settings.html");
const CELLS_HTML: &[u8] = include_bytes!("html/cells.html");
// const CELLS_HTML: &[u8] = &[80, 60];
const RXBUF: usize = 512;
const TXBUF: usize = 512 * 8;

#[derive(Serialize, Deserialize, Debug, Format, Clone, Copy)]
pub enum FileName {
    Index,
    Settings,
}

#[cfg(feature = "nvs")]
impl FileName {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            FileName::Index => b"index.html",
            FileName::Settings => b"settings.html",
        }
    }
    pub fn from_bytes(filename: &[u8]) -> Result<FileName, StmError> {
        match filename {
            b"index.html" => Ok(FileName::Index),
            b"settings.html" => Ok(FileName::Settings),
            _ => Err(StmError::InvalidFileName),
        }
    }
}

#[embassy_executor::task]
pub async fn http_net_task(stack: &'static Stack<EthDevice>) {
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Waiting to get IP address...");
    loop {
        if let Some(_config) = stack.config_v4() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("Spawning HTTP workers");

    unwrap!(Spawner::for_current_executor()
        .await
        .spawn(spawned_http(stack, 1)));
    unwrap!(Spawner::for_current_executor()
        .await
        .spawn(spawned_http(stack, 2)));
    unwrap!(Spawner::for_current_executor()
        .await
        .spawn(spawned_http(stack, 3)));
}

#[embassy_executor::task(pool_size = 3)]
async fn spawned_http(stack: &'static Stack<EthDevice>, num: u8) {
    info!("[{}] Spawning HTTP socket", num);
    let mut rx_buffer = [0; RXBUF];
    let mut tx_buffer = [0; TXBUF];
    let mut socket: TcpSocket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(2)));
    loop {
        info!("[{}] Wait for connection...", num);
        let r = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 80,
            })
            .await;
        info!("[{}] Connected...", num);

        if let Err(e) = r {
            info!("[{}] connect error: {:?}", num, e);
            Timer::after(Duration::from_millis(100)).await;
            socket.close();
            info!("[{}] TCP socket closed", num);
            Timer::after(Duration::from_millis(100)).await;
            socket.abort();
            continue;
        }
        let mut buffer = [0u8; 1024];
        let mut pos = 0;
        loop {
            let buf = match socket.read(&mut buffer).await {
                Ok(0) => {
                    info!("[{}] read EOF", num);
                    break;
                }
                Ok(len) => {
                    let out = &buffer[pos..(pos + len)];
                    pos += len;
                    out
                }
                Err(e) => {
                    error!("[{}] socket.read {}", num, e);
                    break;
                }
            };

            let mut headers = [httparse::EMPTY_HEADER; 40];
            let mut req = httparse::Request::new(&mut headers);
            let res = req.parse(buf);

            let _req_type = HttpRequestType::decode(req.method);

            if let Err(e) = res {
                error!("[{}] parse error {}", num, Debug2Format(&e));
                break;
            }
            if res.unwrap().is_partial() {
                continue;
            }
            if req.path.is_none() {
                // respond with index.html
                match socket.write("FOO".as_bytes()).await {
                    Ok(_) => info!("HTTP FOO sent"),
                    Err(e) => error!("TCP Write error {}", e),
                }
                break;
            }

            let this = match Url::parse(req.path.unwrap()) {
                Ok(u) => u,
                Err(e) => {
                    error!("[{}] Error parsing url path, {}", num, Debug2Format(&e)); // remove this
                    break;
                }
            };

            debug!("[{}] Found path = {}", num, this.path);

            let mut buf = [0u8; 1024 * 12];
            let mut response = ByteMutWriter::new(&mut buf[..]);

            // Set headers for html and json
            match this.path {
                Some("/favicon.ico") => break,
                Some("/api/config") => {
                    let config = CONFIG.lock().await;
                    let a = json::to_string(&*config);
                    let a = a.as_bytes();
                    if let Ok(r) = construct_response(a, HttpType::Json, &mut response) {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
                Some("/api/cells") => {
                    let bms = *BMS.lock().await;
                    let mut buffer = [0u8; 672];
                    let stats_str = cell_stats(&mut buffer, bms).unwrap();
                    if let Ok(r) = construct_response(stats_str, HttpType::Plain, &mut response) {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
                Some("/api/bms") => {
                    let bms: BmsSerialise = (*BMS.lock().await).into();
                    let a = json::to_string(&bms);
                    let a = a.as_bytes();
                    if let Ok(r) = construct_response(a, HttpType::Json, &mut response) {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
                Some("/settings") | None => {
                    //send settings page
                    if let Ok(r) = construct_response(SETTINGS_HTML, HttpType::Html, &mut response)
                    {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
                Some("/cells") => {
                    //send cells graph
                    if let Ok(r) = construct_response(CELLS_HTML, HttpType::Html, &mut response) {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
                Some(_) => {
                    //send index
                    if let Ok(r) = construct_response(INDEX_HTML, HttpType::Html, &mut response) {
                        if let Err(e) = socket.write(&r.buf[..r.cursor()]).await {
                            error!("[{}] TCP Write error {}", num, e)
                        }
                    }
                }
            }
            Timer::after(Duration::from_millis(100)).await;
            socket.close();
            info!("[{}] TCP socket closed", num);
            Timer::after(Duration::from_millis(100)).await;
            socket.abort();
            break;
        }
    }
}

enum HttpType {
    Json,
    Html,
    Plain,
}

impl HttpType {
    pub fn as_str(&self) -> &str {
        match self {
            HttpType::Json => "application/json",
            HttpType::Html => "text/html",
            HttpType::Plain => "text/plain",
        }
    }
}

#[derive(Debug)]
pub enum HttpError {
    Error,
}
impl core::error::Error for HttpError {}
impl core::fmt::Display for HttpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HttpError::Error => write!(f, "Invalid data"),
        }
    }
}

// buf: [u8; 96 * 7], // 96 * ["1234,1,"]

fn cell_stats(buf: &mut [u8], bms: bms_standard::Bms) -> Result<&[u8], HttpError> {
    use core::fmt::Write;
    let cursor = {
        let mut writer = ByteMutWriter::new(buf);

        for (&num, &boolean) in bms.cell_mv.0.iter().zip(bms.bal_cells.iter()) {
            write!(&mut writer, "{},{},", num, boolean as u8).map_err(|_e| HttpError::Error)?;
        }

        writer.cursor()
    };
    Ok(&buf[..cursor - 1]) // trim trailing comma
}

// use core::str;
#[derive(Serialize)]
struct Message {
    message: &'static str,
}

// Function to construct the HTTP response
fn construct_response<'a>(
    body: &'a [u8],
    h: HttpType,
    response: &'a mut ByteMutWriter<'a>,
) -> Result<&'a mut ByteMutWriter<'a>, core::fmt::Error> {
    use core::fmt::Write;
    use core::str;
    write!(
        response,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        h.as_str(),
        body.len(),
        str::from_utf8(body).unwrap()
    )?;
    Ok(response)
}

pub enum HttpRequestType {
    Get,
    Post,
    Invalid,
}
impl HttpRequestType {
    pub fn decode(method: Option<&str>) -> Result<HttpRequestType, StmError> {
        debug!("HTTP req method: {}", method);
        if method.is_none() {
            return Err(StmError::InvalidHttpRequest);
        }
        Ok(HttpRequestType::from_str(method.unwrap()))
    }
    fn from_str(slice: &str) -> HttpRequestType {
        use HttpRequestType::*;
        match slice {
            "GET" => Get,
            "POST" => Post,
            _ => Invalid,
        }
    }
}
