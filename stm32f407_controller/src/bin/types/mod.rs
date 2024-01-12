#![allow(dead_code)]

use crate::ETH;
use embassy_net::Stack;
use embassy_stm32::eth::generic_smi::GenericSMI;
use embassy_stm32::peripherals::*;
use embassy_stm32::usart::Uart;
use embassy_stm32::{can::bxcan::Frame, eth::Ethernet};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex as _Mutex, channel::Channel, mutex::Mutex,
    signal::Signal,
};

use embassy_time::Instant;
pub const FRAME_BUFFER: usize = 10;

pub type InverterChannelRx = Channel<_Mutex, Frame, FRAME_BUFFER>;
pub type InverterChannelTx = Channel<_Mutex, Frame, FRAME_BUFFER>;
pub type BmsChannelRx = Channel<_Mutex, Frame, FRAME_BUFFER>;
pub type BmsChannelTx = Channel<_Mutex, Frame, FRAME_BUFFER>;
pub type Elapsed = Mutex<_Mutex, Option<Instant>>;
pub type MutexType<T> = embassy_sync::mutex::Mutex<_Mutex, T>;
pub type Status = Signal<_Mutex, bool>;
pub type LedCommandType = Signal<_Mutex, crate::tasks::leds::LedCommand>;
pub type EpochType = Signal<_Mutex, embassy_stm32::rtc::DateTime>;

pub type EthDevice = Ethernet<'static, ETH, GenericSMI>;
pub type StackType = &'static Stack<EthDevice>;

pub type RS485<'a> = Uart<'a, USART2, DMA1_CH6, DMA1_CH5>;
pub type RS232<'a> = Uart<'a, USART1, DMA2_CH0, DMA2_CH1>;
pub type Usart6Type<'a> = Uart<'a, USART6, DMA2_CH7, DMA2_CH2>;
