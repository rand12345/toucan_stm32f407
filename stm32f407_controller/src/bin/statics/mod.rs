#[cfg(feature = "mqtt")]
use crate::tasks::mqtt::MqttFormat;
use crate::{
    config::{Config, GlobalState, MqttConfig, NetConfig},
    types::*,
};
use embassy_sync::{channel::Channel, mutex::Mutex, signal::Signal};
use lazy_static::lazy_static;

pub static INVERTER_CHANNEL_RX: InverterChannelRx = Channel::new();
pub static INVERTER_CHANNEL_TX: InverterChannelTx = Channel::new();
pub static BMS_CHANNEL_RX: BmsChannelRx = Channel::new();
pub static BMS_CHANNEL_TX: BmsChannelTx = Channel::new();
pub static CAN_READY: Status = Signal::new();

#[cfg(any(feature = "ze40", feature = "ze50", feature = "tesla_m3"))]
pub static LAST_BMS_MESSAGE: Elapsed = Mutex::new(None);
#[cfg(any(feature = "ze40", feature = "ze50", feature = "tesla_m3"))]
pub static WDT: Status = Signal::new();
pub static CONTACTOR_STATE: Status = Signal::new();
#[cfg(feature = "mqtt")]
pub static SEND_MQTT: Status = Signal::new();
pub static LED_COMMAND: LedCommandType = Signal::new();

#[cfg(feature = "ntp")]
pub static UTC_NOW: EpochType = Signal::new();
// #[cfg(any(feature = "ze40"))]

lazy_static! {
    // thin this down - use singletons from main?
    pub static ref NETCONFIG: MutexType<NetConfig> = Mutex::new(NetConfig::new(true, None, None, None, None));
    pub static ref MQTTCONFIG: MutexType<MqttConfig> = Mutex::new(MqttConfig::default());


    pub static ref CONFIG: MutexType<Config> = Mutex::new(Config::default());
    pub static ref GLOBALSTATE: MutexType<GlobalState> = Mutex::new(GlobalState::default());
    pub static ref BMS: MutexType<bms_standard::Bms> = Mutex::new(bms_standard::Bms::new(bms_standard::Config::default()));
}
#[cfg(feature = "mqtt")]
lazy_static! {
    pub static ref MQTTFMT: MutexType<MqttFormat> = Mutex::new(MqttFormat::default());
}

// #[cfg(feature = "modbus_bridge")]
pub const LAST_READING_TIMEOUT_SECS: u64 = 10; // move to config

#[macro_export]
macro_rules! static_buf {
    ($T:ty $(,)?) => {{
        // Statically allocate a read-write buffer for the value without
        // actually writing anything, as well as a flag to track if
        // this memory has been initialized yet.
        static mut BUF: (core::mem::MaybeUninit<$T>, bool) =
            (core::mem::MaybeUninit::uninit(), false);

        // To minimize the amount of code duplicated across every invocation
        // of this macro, all of the logic for checking if the buffer has been
        // used is contained within the static_buf_check_used function,
        // which panics if the passed boolean has been used and sets the
        // boolean to true otherwise.
        $crate::statics::static_buf_check_used(&mut BUF.1);

        // If we get to this point we can wrap our buffer to be eventually
        // initialized.
        &mut BUF.0
    }};
}
#[macro_export]
macro_rules! static_init {
    ($T:ty, $e:expr $(,)?) => {{
        let mut buf = $crate::static_buf!($T);
        buf.write($e)
    }};
}
