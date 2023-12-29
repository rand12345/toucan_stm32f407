#[cfg(feature = "nvs")]
use heapless::String;
// use defmt::Format;
#[cfg(feature = "nvs")]
use embassy_sync::pubsub::*;
use rust_mqtt::packet::v5::publish_packet::QualityOfService;

#[cfg(feature = "nvs")]
use crate::{config, web};
// use heapless::String;

/*
    Todo:
    MessageBus for passing:
        KV Strings NVS data
        Debug messages to USB
        Getting HTTP data from flash (poor mans FS)
        Getting/Pushing data to NVS (HTML, CSS, BMS/NET settings)
        Sending/Receiving MQTT
        Network settings
*/

#[cfg(feature = "nvs")]
pub type MessageBusType<'a> =
    PubSubChannel<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, Message, 10, 10, 10>;

#[cfg(feature = "nvs")]
pub static MESSAGEBUS: MessageBusType = PubSubChannel::new();

// #[derive(Clone, Format, Debug)]
// pub enum Request {
//     Config,
//     MqttConfig,
//     NetConfig,
// }
// impl Request {
//     pub fn as_bytes(&self) -> &[u8] {
//         match self {
//             Request::Config => b"config",
//             Request::MqttConfig => b"mqttconfig",
//             Request::NetConfig => b"netconfig",
//         }
//     }
// }
#[derive(Clone, Debug)]
pub struct MqttMessage {
    pub topic: heapless::String<50>,
    pub payload: heapless::String<512>,
    pub qos: QualityOfService,
    pub retain: bool,
}

#[cfg(feature = "nvs")]
#[derive(Clone, Debug)]
pub enum RequestType {
    Nvs(config::ConfigName),
    File(web::http::FileName),
}

#[cfg(feature = "nvs")]
#[derive(Clone, Debug)]
pub enum ResponseData {
    NvsData(Option<String<1024>>),
}

#[cfg(feature = "nvs")]
#[derive(Clone, Debug)]
pub enum Message {
    EraseAll,
    Request(RequestType),
    Store(RequestType, String<1024>),
    Respond(ResponseData),
    Mqtt(MqttMessage),
}
