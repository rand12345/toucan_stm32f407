use super::ModbusError;
use crate::types::RS485;
use core::sync::atomic::{AtomicU16, Ordering};
use crc16::{State, MODBUS};
use defmt::{error, info};
use embassy_net::{
    tcp::{self, TcpSocket},
    IpEndpoint, Ipv4Address,
};
use embassy_stm32::{gpio::Output, peripherals::PD7, usart};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
#[cfg(feature = "modbus_bridge")]
use heapless::Vec;
use no_std_net::SocketAddr;

static COUNTER: AtomicU16 = AtomicU16::new(0);
#[cfg(feature = "modbus_bridge")]
const MODBUS_ERROR_VAL: u8 = 80;
pub const MODBUS_VEC_SIZE: usize = 512;
pub const MODBUS_TCP_REQ_VEC_SIZE: usize = 12;
pub const MODBUS_RTU_REQ_VEC_SIZE: usize = 8;

pub type TcpRequestPayload = heapless::Vec<u8, MODBUS_TCP_REQ_VEC_SIZE>;
pub type RtuRequestPayload = heapless::Vec<u8, MODBUS_RTU_REQ_VEC_SIZE>;
pub type ResponsePayload = heapless::Vec<u8, MODBUS_VEC_SIZE>;

macro_rules! debug_data {
    ($data:expr) => {{
        use core::fmt::Write;
        use heapless::String;
        let mut st: String<64> = String::new();
        for &byte in $data.iter() {
            let _ = write!(st, "{:02x} ", byte); // Use write! macro for no_std environment
        }
        st
    }};
}

#[cfg(feature = "modbus_client")]
pub fn increment_transaction_id() -> u16 {
    loop {
        let current = COUNTER.load(Ordering::Relaxed);
        if current == u16::MAX {
            COUNTER.store(0, Ordering::Relaxed);
            return 0;
        } else {
            let new = current + 1;
            match COUNTER.compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return new,
                Err(_) => continue,
            }
        }
    }
}

pub fn get_transaction_id() -> u16 {
    COUNTER.load(Ordering::Relaxed)
}
#[cfg(feature = "modbus_bridge")]
pub fn set_transaction_id(val: u16) {
    COUNTER.store(val, Ordering::Relaxed);
}

/// Rtu mode depends on Tcp mode.
pub fn mode_builder(tcp_mode: ModbusTcpMode) -> (ModbusRtuMode, ModbusTcpMode) {
    match tcp_mode {
        ModbusTcpMode::Client(sa) => (ModbusRtuMode::Client, ModbusTcpMode::Client(sa)),
        ModbusTcpMode::Server(port) => (ModbusRtuMode::Server, ModbusTcpMode::Server(port)),
    }
}
pub enum ModbusRtuMode {
    Client,
    Server,
}

///Client makes a conenction to a remote TCP modbus device, Server listens for incoming modbus Tcp requests
#[derive(Copy, Clone)]
pub enum ModbusTcpMode {
    // Client ip:port
    Client(SocketAddr),
    // local port
    Server(SocketAddr),
}
impl ModbusTcpMode {
    pub fn get_port(&self) -> u16 {
        match self {
            ModbusTcpMode::Client(socket) => match socket {
                SocketAddr::V4(ip) => ip.port(),
                SocketAddr::V6(ip) => ip.port(),
            },
            ModbusTcpMode::Server(p) => p.port(),
        }
    }
    pub fn get_ip(&self) -> Option<&SocketAddr> {
        match self {
            ModbusTcpMode::Client(socket) => Some(socket),
            ModbusTcpMode::Server(_) => None,
        }
    }
}
impl From<ModbusTcpMode> for IpEndpoint {
    fn from(val: ModbusTcpMode) -> Self {
        match val {
            ModbusTcpMode::Client(socket) => match socket {
                SocketAddr::V4(ip) => {
                    IpEndpoint::from((Ipv4Address::from_bytes(&ip.ip().octets()), ip.port()))
                }
                SocketAddr::V6(_ip) => {
                    todo!()
                }
            },
            ModbusTcpMode::Server(p) => IpEndpoint::from((Ipv4Address::default(), p.port())),
        }
    }
}

pub fn create_crc(s: &[u8]) -> [u8; 2] {
    let mut crc = State::<MODBUS>::new();
    crc.update(s);
    crc.get().to_le_bytes()
}

// Switch  consts aound
pub struct ModbusRtu<'a> {
    serial: RS485<'static>,
    tx_en: Output<'a, PD7>,
    #[allow(dead_code)]
    response: ResponsePayload,
    #[allow(dead_code)]
    request: RtuRequestPayload,
    #[allow(dead_code)]
    rtumode: ModbusRtuMode,
}

impl<'a> ModbusRtu<'a> {
    pub fn new(serial: RS485<'static>, tx_en: Output<'static, PD7>, mode: ModbusTcpMode) -> Self {
        let data = ModbusRawData::<MODBUS_VEC_SIZE, MODBUS_RTU_REQ_VEC_SIZE>::default();
        // RTU mode depends on TCP mode
        let (rtumode, _tcpmode) = mode_builder(mode);
        ModbusRtu {
            serial,
            tx_en,
            response: data.0,
            request: data.1,
            rtumode,
        }
    }

    #[cfg(feature = "modbus_client")]
    pub fn convert_request(
        &mut self,
        converter: impl Fn(&RtuRequestPayload) -> Result<TcpRequestPayload, ModbusError>,
    ) -> Result<TcpRequestPayload, ModbusError> {
        converter(&self.request)
    }
    #[cfg(feature = "modbus_bridge")]
    pub fn convert_response(
        &mut self,
        converter: impl Fn(&ResponsePayload) -> Result<ResponsePayload, ModbusError>,
    ) -> Result<ResponsePayload, ModbusError> {
        converter(&self.response)
    }

    #[cfg(feature = "modbus_client")]
    pub async fn send(&mut self, payload: ResponsePayload) -> Result<&mut Self, ModbusError> {
        self.transmit(payload.as_ref()).await?;
        Ok(self)
    }

    #[cfg(feature = "modbus_client")]
    /// Listen for RTU request
    pub async fn listen(&mut self) -> Result<&mut Self, ModbusError> {
        self.request.clear();
        let mut buf = [0; 8];
        if let Err(e) = self.serial.read(&mut buf).await {
            error!("Rtu Read {}", e);
            return Err(e.into());
        };
        self.request.extend(buf);
        crc_check(&self.request)?;
        Ok(self)
    }
    #[cfg(feature = "modbus_bridge")]
    /// Forward request and return response
    pub async fn send_and_receive(
        &mut self,
        rtu_req: Vec<u8, 8>,
    ) -> Result<&mut Self, ModbusError> {
        self.response.clear();
        self.transmit(&rtu_req).await?;
        self.receive().await?;
        Ok(self)
    }
    /// Transmit RTU payload
    async fn transmit(&mut self, payload: &[u8]) -> Result<(), usart::Error> {
        self.serial.flush().await?;
        self.tx_en.set_high();
        self.serial.write(payload).await?;
        info!("RTU Send: {:02x}", debug_data!(payload));
        self.serial.flush().await?;
        self.tx_en.set_low();
        Ok(())
    }

    #[cfg(feature = "modbus_bridge")]
    /// Receive RTU response
    async fn receive(&mut self) -> Result<(), ModbusError> {
        self.response.clear();
        let mut buf = [0; 3];
        self.serial.read(&mut buf).await?;

        let pll = match payload_length(&buf) {
            None => return Err(ModbusError::RtuPayloadTooShort),
            Some(n) => n,
        };
        self.response.extend(buf);

        let mut buf = [0; 1];
        for _ in 0..pll {
            self.serial
                .read_until_idle(&mut buf)
                .await
                .map_err(|_| ModbusError::RtuRxFail)
                .and(self.response.push(buf[0]).map_err(|_| ModbusError::Push))?;
        }

        info!("RTU Recv: {:02x}", debug_data!(self.response));

        crc_check(&self.response)?;
        Ok(())
    }
}

fn crc_check(payload: &[u8]) -> Result<(), ModbusError> {
    if payload.len() < 4 {
        return Err(ModbusError::RtuRxFail);
    }
    let (response, crc_check) = payload.split_at(payload.len() - 2);
    let mut crc = State::<MODBUS>::new();
    crc.update(response);
    let calc_crc = crc.get().to_le_bytes();

    if crc_check == calc_crc {
        Ok(())
    } else {
        Err(ModbusError::CrcRx)
    }
}

/// R = Rx size, T = Tx size
#[derive(Default)]
pub struct ModbusRawData<const RES: usize, const REQ: usize>(
    heapless::Vec<u8, RES>,
    heapless::Vec<u8, REQ>,
);

pub struct ModbusTcp<'a> {
    socket: TcpSocket<'a>,
    timeout: u64,
    #[allow(dead_code)]
    response: ResponsePayload,
    #[allow(dead_code)]
    request: TcpRequestPayload,
    #[allow(dead_code)]
    tcpmode: ModbusTcpMode,
}

impl<'a> ModbusTcp<'a> {
    pub fn new(socket: TcpSocket<'a>, tcpmode: ModbusTcpMode, timeout: u64) -> Self {
        let data = ModbusRawData::<MODBUS_VEC_SIZE, MODBUS_TCP_REQ_VEC_SIZE>::default();
        ModbusTcp {
            socket,
            timeout,
            response: data.0,
            request: data.1,
            tcpmode,
        }
    }
    #[cfg(feature = "modbus_bridge")]
    /// Converts TCP request to Rtu request
    pub fn convert_request(
        &mut self,
        converter: impl Fn(&TcpRequestPayload) -> Result<RtuRequestPayload, ModbusError>,
    ) -> Result<RtuRequestPayload, ModbusError> {
        converter(&self.request)
    }
    #[cfg(feature = "modbus_client")]
    pub fn convert_response(
        &mut self,
        converter: impl Fn(&ResponsePayload) -> Result<ResponsePayload, ModbusError>,
    ) -> Result<ResponsePayload, ModbusError> {
        converter(&self.response)
    }
    #[cfg(feature = "modbus_bridge")]
    pub async fn send(&mut self, payload: ResponsePayload) -> Result<&mut Self, ModbusError> {
        // self.response = heapless::Vec::from_slice(&payload).unwrap();
        self.transmit(payload.as_ref()).await?;
        Ok(self)
    }

    #[cfg(feature = "modbus_bridge")]
    pub async fn listen(&mut self) -> Result<&mut Self, ModbusError> {
        use embedded_io_async::Read;

        self.request.clear();
        let mut read_buf = [0u8; 12];
        self.socket.read_exact(&mut read_buf).await?;
        self.request.extend(read_buf);
        Ok(self)
    }

    #[cfg(feature = "modbus_client")]
    /// Forward request and return response
    pub async fn send_and_receive(
        &mut self,
        payload: TcpRequestPayload,
    ) -> Result<&mut Self, ModbusError> {
        self.response.clear(); //  clear rx buffer
        self.transmit(&payload).await?;
        let r = self.receive().await?;
        defmt::warn!("TCP Recv: {}", r);
        Ok(self)
    }
    async fn transmit(&mut self, payload: &[u8]) -> Result<usize, tcp::Error> {
        self.socket.write(payload).await
    }

    #[cfg(feature = "modbus_client")]
    async fn receive(&mut self) -> Result<usize, ModbusError> {
        use defmt::warn;
        self.response.clear();
        let mut buf = [0; 6];
        self.socket.read(&mut buf).await?;
        if buf[..2] != get_transaction_id().to_be_bytes() {
            warn!("bad transaction id")
        }
        self.response.extend(buf);
        let bytes_len = u16::from_be_bytes([buf[4], buf[5]]);
        let mut buf = [0; 1];
        for _ in 0..bytes_len {
            self.socket.read(&mut buf).await?;
            self.response.push(buf[0]).map_err(|_| ModbusError::Push)?;
        }
        warn!("read {:02x}", self.response);
        Ok(self.response.len())
    }

    pub async fn connected_client(&mut self) -> Option<embassy_net::IpEndpoint> {
        self.socket.remote_endpoint()
    }
    #[cfg(feature = "modbus_bridge")]
    pub async fn wait_connection(&mut self) -> Result<&mut Self, ModbusError> {
        self.socket
            .accept(embassy_net::IpListenEndpoint {
                addr: None,
                port: self.tcpmode.get_port(),
            })
            .await?;
        Ok(self)
    }
    pub async fn reset(&mut self) -> &mut Self {
        match self.socket.state() {
            embassy_net::tcp::State::Closed => (),
            _ => {
                Timer::after(Duration::from_millis(self.timeout)).await;
                self.socket.close();
                info!("Modbus TCP socket closed");
                Timer::after(Duration::from_millis(100)).await;
                self.socket.abort();
            }
        };
        self
    }

    #[cfg(feature = "modbus_client")]
    pub async fn connect(&mut self) -> Result<(), ModbusError> {
        let ip: embassy_net::IpEndpoint = self.tcpmode.into();
        info!("Modbus TCP connecting {}", ip);
        self.socket.connect(ip).await?;
        info!("Connected!");
        Ok(())
    }
}

#[cfg(feature = "modbus_bridge")]
fn is_rtu_error(s: &[u8]) -> bool {
    if let Some(v) = s.get(2) {
        v >= &MODBUS_ERROR_VAL
    } else {
        false
    }
}
#[cfg(feature = "modbus_bridge")]
fn payload_length(s: &[u8]) -> Option<usize> {
    s.get(2).map(|bytes_len| (bytes_len + 2) as usize)
}
