#![allow(dead_code)]
use defmt::Format;
use embassy_net::tcp::{self, AcceptError, ConnectError};
use embassy_stm32::usart;

#[derive(Debug, Format)]
pub enum ModbusError {
    Rtu,
    Tcp,
    Slice,
    Push,
    RtuTimeout,
    RtuRxFail,
    CrcRx,
    ReadExactError,
    TcpRxFail(usize),
    RtuIlligal,
    TcpConnect(ConnectError),
    TcpAccept,
    InvalidTransactionId,
    RtuPayloadTooShort,
    InvalidFilter,
    ConversionSlice,
}
impl core::error::Error for ModbusError {}
impl core::fmt::Display for ModbusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ModbusError::Rtu => write!(f, "RTU error"),
            ModbusError::Tcp => write!(f, "TCP error"),
            ModbusError::Slice => write!(f, "Slice error"),
            ModbusError::Push => write!(f, "Push error"),
            ModbusError::RtuTimeout => write!(f, "RtuTimeout error"),
            ModbusError::RtuRxFail => write!(f, "RtuRxFail error"),
            ModbusError::CrcRx => write!(f, "CrcRxFail error"),
            ModbusError::ReadExactError => write!(f, "ReadExactError error"),
            ModbusError::TcpRxFail(v) => write!(f, "TcpRxFail error {}", v),
            ModbusError::RtuIlligal => write!(f, "RtuIlligal error"),
            ModbusError::TcpConnect(e) => write!(f, "Tcp Connect error {:?}", e),
            ModbusError::TcpAccept => write!(f, "Tcp Accept error"),
            ModbusError::InvalidTransactionId => write!(f, "Tcp Transaction Counter mismatch"),
            ModbusError::RtuPayloadTooShort => write!(f, "Rtu payload is too short"),
            ModbusError::InvalidFilter => write!(f, "Invalid Modbus filter match"),
            ModbusError::ConversionSlice => write!(f, "Conversion slice error"),
        }
    }
}

impl From<usart::Error> for ModbusError {
    fn from(_: usart::Error) -> Self {
        ModbusError::Rtu
    }
}

impl From<tcp::Error> for ModbusError {
    fn from(_: tcp::Error) -> Self {
        ModbusError::Tcp
    }
}

impl From<ConnectError> for ModbusError {
    fn from(e: ConnectError) -> Self {
        ModbusError::TcpConnect(e)
    }
}

impl From<embedded_io_async::ReadExactError<tcp::Error>> for ModbusError {
    fn from(_: embedded_io_async::ReadExactError<tcp::Error>) -> Self {
        ModbusError::ReadExactError
    }
}

impl From<AcceptError> for ModbusError {
    fn from(_: AcceptError) -> Self {
        ModbusError::TcpAccept
    }
}
