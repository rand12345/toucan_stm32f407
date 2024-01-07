use crate::{statics::LED_COMMAND, types::RS485};

use super::{modbus_tcp_gateway::RX_TIMEOUT_BYTE, ModbusError};

use defmt::{error, info, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{
    tcp::{self, AcceptError, TcpSocket},
    IpListenEndpoint,
};
use embassy_stm32::{gpio::Output, peripherals::PD7, usart};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::Vec as hVec;

use crate::tasks::leds::Led::Led3;
use crate::tasks::leds::LedCommand::{Off, On};
use crc16::{State, MODBUS};

pub type TcpRxPayload = [u8; 12];
pub type TcpTxPayload = hVec<u8, 512>;
pub type RtuRxPayload = hVec<u8, 512>;
pub type RtuTxPayload = [u8; 8];
const MODBUS_ERROR_VAL: u8 = 0x80;
#[derive(Default)]
pub struct RtuData(RtuRxPayload, RtuTxPayload);

#[derive(Default)]
pub struct TcpData(TcpRxPayload, TcpTxPayload);
const DEBUG: bool = true;

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
    tcp_port: u16,
    timeout: u64,
    data: TcpData,
}

impl<'a> ModbusTcp<'a> {
    pub fn new(socket: TcpSocket<'a>, tcp_port: impl Into<u16>, timeout_ms: u64) -> Self {
        ModbusTcp {
            socket,
            tcp_port: tcp_port.into(),
            timeout: timeout_ms,
            data: TcpData::default(),
        }
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

    pub async fn wait_connection(&mut self) -> Result<&mut Self, AcceptError> {
        self.socket
            .accept(IpListenEndpoint {
                addr: None,
                port: self.tcp_port,
            })
            .await?;
        Ok(self)
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
    async fn write(&mut self) -> Result<usize, tcp::Error> {
        let data = self.data.1.as_slice();
        self.socket.write(data).await
    }
    pub async fn connected_client(&mut self) -> Option<embassy_net::IpEndpoint> {
        self.socket.remote_endpoint()
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
    fn update_from_tcp_request(&mut self, tcp_rx: &TcpRxPayload) {
        let mut crc = State::<MODBUS>::new();
        crc.update(&tcp_rx[6..12]);
        self.data.1[0..6].copy_from_slice(&tcp_rx[6..12]);
        self.data.1[6..8].copy_from_slice(&crc.get().to_le_bytes());
    }

    fn is_rtu_error(&self) -> bool {
        if let Some(v) = self.data.0.get(2) {
            v >= &MODBUS_ERROR_VAL
        } else {
            false
        }
    }

    fn payload_length(&self) -> Option<usize> {
        self.data.0.get(2).map(|bytes_len| (bytes_len + 2) as usize)
    }

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
        info!("RTU write {:x}", debug_data!(self.data.1));
        self.serial.flush().await?;
        self.tx_en.set_low();
        Ok(())
    }

    async fn receive(&mut self) -> Result<(), ModbusError> {
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
}

fn crc_check(payload: &RtuRxPayload) -> Result<(), ModbusError> {
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
