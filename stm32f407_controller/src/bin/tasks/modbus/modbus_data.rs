use crate::{statics::LED_COMMAND, types::RS485};

use super::{models::ModbusMode, ModbusError};
use defmt::{error, info};

use embassy_net::tcp::{self, TcpSocket};
#[cfg(feature = "modbus_bridge")]
use embassy_net::{tcp::AcceptError, IpListenEndpoint};

use embassy_stm32::{gpio::Output, peripherals::PD7, usart};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::Vec as hVec;
// use no_std_net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use crate::tasks::leds::Led::Led3;
use crate::tasks::leds::LedCommand::{Off, On};
use crc16::{State, MODBUS};

#[cfg(feature = "modbus_bridge")]
pub type TcpRxPayload = [u8; 12];
#[cfg(feature = "modbus_bridge")]
pub type TcpTxPayload = hVec<u8, 512>;
#[cfg(feature = "modbus_bridge")]
pub type RtuRxPayload = hVec<u8, 512>;
#[cfg(feature = "modbus_bridge")]
pub type RtuTxPayload = [u8; 8];

#[cfg(feature = "modbus_bridge")]
const MODBUS_ERROR_VAL: u8 = 0x80;

#[cfg(feature = "modbus_client")]
pub type TcpRxPayload = hVec<u8, 512>;
#[cfg(feature = "modbus_client")]
pub type TcpTxPayload = [u8; 12];
#[cfg(feature = "modbus_client")]
pub type RtuRxPayload = [u8; 12];
#[cfg(feature = "modbus_client")]
pub type RtuTxPayload = hVec<u8, 512>;

#[derive(Default)]
pub struct RtuData(RtuRxPayload, RtuTxPayload);

#[derive(Default)]
pub struct TcpData(TcpRxPayload, TcpTxPayload);

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

#[cfg(feature = "modbus_bridge")]
impl TcpData {
    fn update_from_rtu_error(&mut self, rtu_data: &RtuRxPayload) -> Result<(), ModbusError> {
        let rtu_data = rtu_data.as_slice();
        assert!(self.0.len() >= 5);
        assert!(rtu_data.len() >= 3);
        self.1.clear();
        self.1
            .extend_from_slice(&self.0[0..5])
            .map_err(|_| ModbusError::Slice)?;
        self.1.push(0x3).unwrap();
        self.1
            .extend_from_slice(&rtu_data[0..3])
            .map_err(|_| ModbusError::Slice)?;

        Ok(())
    }

    fn update_from_rtu_response(&mut self, rtu_data: &RtuRxPayload) -> Result<(), ModbusError> {
        let rtu_data = rtu_data.as_slice();
        assert!(rtu_data.len() > 5);
        self.1.clear();
        self.1
            .extend_from_slice(&self.0[0..5])
            .map_err(|_| ModbusError::Slice)?; // tcp header
        self.1
            .push(rtu_data[2] + 3)
            .map_err(|_| ModbusError::Push)?; // len

        self.1.push(rtu_data[0]).map_err(|_| ModbusError::Push)?; // id
        self.1
            .extend_from_slice(&rtu_data[1..(rtu_data.len() - 2)])
            .map_err(|_| ModbusError::Slice)?;
        Ok(())
    }
}

pub struct ModbusTcp<'a> {
    socket: TcpSocket<'a>,
    timeout: u64,
    data: TcpData,
    mode: ModbusMode,
}

impl<'a> ModbusTcp<'a> {
    pub fn new(socket: TcpSocket<'a>, mode: ModbusMode, timeout_ms: u64) -> Self {
        ModbusTcp {
            socket,
            // tcp_port: tcp_port.into(),
            timeout: timeout_ms,
            data: TcpData::default(),
            mode,
        }
    }

    #[cfg(feature = "modbus_client")]
    pub async fn connect(&mut self) -> Result<(), ModbusError> {
        info!("Modbus TCP connecting ");
        let ip: embassy_net::IpEndpoint = self.mode.into();
        self.socket.connect(ip).await?;
        info!("Connected!");
        Ok(())
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
    async fn send_and_receive(&mut self, payload: &RtuRxPayload) -> Result<usize, ModbusError> {
        self.data.0.clear();
        defmt::debug!("TCP Send: {:02x}", payload);
        self.socket.write(payload).await?;
        Ok(self.read().await?)
    }

    async fn read(&mut self) -> Result<usize, tcp::Error> {
        let data = self.data.0.as_mut_slice();
        data.fill(0);
        match self.socket.read(data).await {
            Ok(c) => Ok(c),
            Err(e) => {
                error!("Tcp Read {}", e);
                Err(e)
            }
        }
    }
    pub async fn connected_client(&mut self) -> Option<embassy_net::IpEndpoint> {
        self.socket.remote_endpoint()
    }
    #[cfg(feature = "modbus_bridge")]
    async fn write(&mut self) -> Result<usize, tcp::Error> {
        let data = self.data.1.as_slice();
        self.socket.write(data).await
    }
    #[cfg(feature = "modbus_bridge")]
    pub async fn wait_connection(&mut self) -> Result<&mut Self, AcceptError> {
        self.socket
            .accept(IpListenEndpoint {
                addr: None,
                port: self.mode.get_port(),
            })
            .await?;
        Ok(self)
    }
}

pub struct ModbusRtu<'a> {
    serial: RS485<'static>,
    tx_en: Output<'a, PD7>,
    data: RtuData,
}

impl<'a> ModbusRtu<'a> {
    pub fn new(serial: RS485<'static>, tx_en: Output<'static, PD7>) -> Self {
        ModbusRtu {
            serial,
            tx_en,
            data: RtuData::default(),
        }
    }

    #[cfg(feature = "modbus_client")]
    fn update_from_tcp_response(&mut self, tcp_rx: &TcpRxPayload) -> Result<(), ModbusError> {
        use super::models::get_counter_value;

        self.data.1.clear();

        let rx_id = u16::from_be_bytes([tcp_rx[0], tcp_rx[1]]);
        if get_counter_value() != rx_id {
            error!("Counter value mismatch");
            return Err(ModbusError::InvalidTransactionId);
        }
        // tcp_rx.push(5).unwrap(); //len
        let bytes_value_len: usize = tcp_rx[5].into();
        // tcp_rx.push(2).unwrap(); // slave address
        // tcp_rx.push(3).unwrap(); // function code
        // tcp_rx.push(2).unwrap(); // Byte count
        // tcp_rx.push(1).unwrap(); // data h
        // tcp_rx.push(2).unwrap(); // data l
        self.data
            .1
            .extend(tcp_rx.iter().skip(6).take(bytes_value_len).copied());
        let mut crc = State::<MODBUS>::new();
        crc.update(&self.data.1[..bytes_value_len]);
        self.data
            .1
            .extend_from_slice(&crc.get().to_le_bytes())
            .map_err(|_| ModbusError::Slice)?;
        Ok(())
    }

    #[cfg(feature = "modbus_bridge")]
    fn update_from_tcp_request(&mut self, tcp_rx: &TcpRxPayload) {
        let mut crc = State::<MODBUS>::new();
        crc.update(&tcp_rx[6..12]);
        self.data.1[0..6].copy_from_slice(&tcp_rx[6..12]);
        self.data.1[6..8].copy_from_slice(&crc.get().to_le_bytes());
    }

    #[cfg(feature = "modbus_bridge")]
    fn is_rtu_error(&self) -> bool {
        if let Some(v) = self.data.0.get(2) {
            v >= &MODBUS_ERROR_VAL
        } else {
            false
        }
    }

    async fn listen(&mut self) -> Result<usize, usart::Error> {
        match self.serial.read_until_idle(&mut self.data.0).await {
            Ok(_c) => Ok(_c),
            Err(e) => {
                error!("Rtu Read {}", e);
                Err(e)
            }
        }
    }

    #[cfg(feature = "modbus_bridge")]
    fn payload_length(&self) -> Option<usize> {
        self.data.0.get(2).map(|bytes_len| (bytes_len + 2) as usize)
    }
    #[cfg(feature = "modbus_bridge")]
    pub async fn send_and_receive(&mut self, tcp_data: &mut TcpData) -> Result<(), ModbusError> {
        self.data.0.clear();
        self.update_from_tcp_request(&tcp_data.0);
        self.transmit().await?;
        self.receive().await
    }

    pub async fn transmit(&mut self) -> Result<(), usart::Error> {
        self.serial.flush().await?;
        self.tx_en.set_high();
        self.serial.write(&self.data.1).await?;
        info!("RTU write {:02x}", debug_data!(self.data.1));
        self.serial.flush().await?;
        self.tx_en.set_low();
        Ok(())
    }

    #[cfg(feature = "modbus_bridge")]
    async fn receive(&mut self) -> Result<(), ModbusError> {
        self.data.0.clear();
        self.listen().await?;

        if self.is_rtu_error() {
            error!("Bad modbus address");
            return Err(ModbusError::RtuIlligal);
        };

        if self.payload_length().is_none() {
            return Err(ModbusError::RtuRxFail);
        }

        if let Err(e) = crc_check(&self.data.0) {
            error!("RTU Read CRC error {}", debug_data!(self.data.0));
            return Err(e);
        };
        Ok(())
    }
    /*
    async fn receive_old(&mut self) -> Result<(), ModbusError> {
        let mut buf = [0u8; 3];
        self.serial.read(&mut buf).await?; // await incoming data

        self.data.0.extend(buf);
        if self.is_rtu_error() {
            error!("Bad modbus address");
            return Err(ModbusError::RtuIlligal);
        };

        if self.payload_length().is_none() {
            return Err(ModbusError::RtuRxFail);
        }

        let mut byte = [0];
        let pll = self.payload_length().unwrap();
        for _ in 0..pll {
            match select(
                self.serial.read_until_idle(&mut byte),
                Timer::after(Duration::from_millis(RX_TIMEOUT_BYTE)),
            )
            .await
            {
                Either::First(Ok(1)) => self.data.0.push(byte[0]).map_err(|_| ModbusError::Push)?,
                Either::First(Ok(_)) => {
                    warn!("Client gone away");
                }
                Either::First(Err(e)) => {
                    error!("Serial read error 2 {}", e);
                    return Err(ModbusError::RtuRxFail);
                }
                Either::Second(_) => {
                    error!("RTU Read timeout");
                    return Err(ModbusError::RtuTimeout);
                }
            };
        }
        if let Err(e) = crc_check(&self.data.0) {
            error!("RTU Read CRC error {}", debug_data!(self.data.0));
            return Err(e);
        };
        Ok(())
    }
    */
}

fn crc_check(payload: &[u8]) -> Result<(), ModbusError> {
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

#[cfg(feature = "modbus_bridge")]
pub async fn process(
    modbus_tcp: &mut ModbusTcp<'_>,
    modbus_rtu: &mut ModbusRtu<'_>,
) -> Result<(), ModbusError> {
    LED_COMMAND.signal(On(Led3));
    if modbus_tcp.read().await? != 12 {
        info!("TCP Read 0");
        return Err(ModbusError::TcpRxFail(0));
    };

    match modbus_rtu.send_and_receive(&mut modbus_tcp.data).await {
        Ok(_) => modbus_tcp
            .data
            .update_from_rtu_response(&modbus_rtu.data.0)?,
        Err(ModbusError::RtuIlligal) => {
            modbus_tcp.data.update_from_rtu_error(&modbus_rtu.data.0)?
        }
        Err(e) => return Err(e),
    };

    LED_COMMAND.signal(Off(Led3));
    let c = modbus_tcp.write().await?;
    info!("Wrote TCP {}", c);
    Ok(())
}
#[cfg(feature = "modbus_client")]
struct ModbusTcpTxFrame {
    mbap: Mbap,
    pdu: ModbusTcpTxPdu,
}

#[cfg(feature = "modbus_client")]
impl From<ModbusTcpTxFrame> for TcpTxPayload {
    fn from(val: ModbusTcpTxFrame) -> Self {
        let mut payload = [0u8; 12];
        payload[0..2].copy_from_slice(&val.mbap.transaction_id);
        payload[2..4].copy_from_slice(&val.mbap.protocol_id);
        payload[4..6].copy_from_slice(&val.mbap.length);
        payload[6..7].copy_from_slice(&[val.pdu.slave_address]);
        payload[7..8].copy_from_slice(&[val.pdu.function]);
        payload[8..10].copy_from_slice(&val.pdu.address.to_be_bytes());
        payload[10..12].copy_from_slice(&val.pdu.quantity.to_be_bytes());
        payload
    }
}

#[cfg(feature = "modbus_client")]
impl TryFrom<ModbusRtuRxPdu> for ModbusTcpTxFrame {
    type Error = ModbusError;

    fn try_from(value: ModbusRtuRxPdu) -> Result<Self, Self::Error> {
        Ok(ModbusTcpTxFrame {
            mbap: Mbap::new()?,
            pdu: value.into(),
        })
    }
}

#[cfg(feature = "modbus_client")]
struct Mbap {
    transaction_id: [u8; 2],
    protocol_id: [u8; 2],
    length: [u8; 2],
    _unit_id: [u8; 1],
}

#[cfg(feature = "modbus_client")]
impl TryFrom<&[u8]> for Mbap {
    type Error = ModbusError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 7 {
            return Err(ModbusError::Tcp);
        }
        Ok(Mbap {
            transaction_id: [value[0], value[1]],
            protocol_id: [value[2], value[3]],
            length: [value[4], value[5]],
            _unit_id: [value[6]],
        })
    }
}
#[cfg(feature = "modbus_client")]
impl Mbap {
    pub fn new() -> Result<Self, ModbusError> {
        let transaction_id = super::models::increment_counter().to_be_bytes();
        Ok(Mbap {
            transaction_id,
            protocol_id: [0, 0],
            length: [0, 6],
            _unit_id: [0],
        })
    }
}
#[cfg(feature = "modbus_client")]
struct ModbusTcpTxPdu {
    slave_address: u8,
    function: u8,
    address: u16,
    quantity: u16,
}
#[cfg(feature = "modbus_client")]
impl From<ModbusRtuRxPdu> for ModbusTcpTxPdu {
    fn from(value: ModbusRtuRxPdu) -> Self {
        ModbusTcpTxPdu {
            slave_address: value.slave_address,
            function: value.function,
            address: value.address,
            quantity: value.quantity,
        }
    }
}
#[cfg(feature = "modbus_client")]
struct ModbusRtuRxPdu {
    slave_address: u8,
    function: u8,
    address: u16,
    quantity: u16,
}
#[cfg(feature = "modbus_client")]
impl TryFrom<&[u8]> for ModbusRtuRxPdu {
    type Error = ModbusError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(ModbusError::Rtu);
        }

        if crc_check(value).is_ok() {
            Ok(ModbusRtuRxPdu {
                slave_address: value[0],
                function: value[1],
                address: u16::from_be_bytes([value[2], value[3]]),
                quantity: u16::from_be_bytes([value[4], value[5]]),
            })
        } else {
            error!("Bad CRC in RTU: {:02x}", value);
            Err(ModbusError::CrcRx)
        }
    }
}

#[cfg(feature = "modbus_client")]
pub async fn process(
    modbus_tcp: &mut ModbusTcp<'_>,
    modbus_rtu: &mut ModbusRtu<'_>,
) -> Result<(), ModbusError> {
    LED_COMMAND.signal(Off(Led3));
    let read = modbus_rtu.listen().await?;
    let rx = ModbusRtuRxPdu::try_from(&modbus_rtu.data.0[..read])?; //checks crc
    let tx = ModbusTcpTxFrame::try_from(rx)?; //convert to TCP PDU

    modbus_tcp.send_and_receive(&tx.into()).await?;
    modbus_rtu.update_from_tcp_response(&modbus_tcp.data.0)?;
    modbus_rtu.transmit().await?;
    info!("Wrote RTU {}", 0);
    LED_COMMAND.signal(On(Led3));
    Ok(())
}
