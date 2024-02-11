use super::home_assistant;
use super::Client;
use defmt::info;
use ha_discovery::{Device, DeviceBuilder, SensorBuilder, Value};
use rust_mqtt::packet::v5::publish_packet::QualityOfService::*;
const BASETOPIC: &str = core::env!("MQTTBASETOPIC");

pub async fn send_discovery(client: &mut Client<'_>) {
    // Alloc issue in this fn
    // Scoping to drop payload Value
    // Need refactoring

    use alloc::string::ToString as _;
    use embassy_time::{Duration, Timer};

    const DELAYMS: u64 = 50; // anti hammering
    info!("Sending Home Assistant auto discovery");
    let topic: &str = "homeassistant/sensor/Toucan_*/config";
    let mut idx = 0..15u8;
    let device = home_assistant::get_device();
    {
        let payload =
            home_assistant::get_sensor("SoC", "%", "{{ value_json.soc }}", "battery", &device);
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }
    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Pack Volts",
            "V",
            "{{ value_json.volts }}",
            "voltage",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Cell Volts High",
            "mV",
            "{{ value_json.cell_mv_high }}",
            "voltage",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Cell Volts Low",
            "mV",
            "{{ value_json.cell_mv_low }}",
            "voltage",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }
    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Cell Temperature High",
            "°C",
            "{{ value_json.cell_temp_high }}",
            "temperature",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Cell Temperature Low",
            "°C",
            "{{ value_json.cell_temp_low }}",
            "temperature",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    if false {
        let payload = home_assistant::get_sensor(
            "Energy Remaining",
            "kWh",
            "{{ value_json.kwh }}",
            "energy",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload =
            home_assistant::get_sensor("Current", "A", "{{ value_json.amps }}", "current", &device);
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload =
            home_assistant::get_sensor("Balancing", "", "{{ value_json.bal }}", "", &device);
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Charge Limit",
            "A",
            "{{ value_json.charge }}",
            "current",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload = home_assistant::get_sensor(
            "Discharge Limit",
            "A",
            "{{ value_json.discharge }}",
            "current",
            &device,
        );
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
    {
        let payload =
            home_assistant::get_sensor("Valid", "", "{{ value_json.valid }}", "", &device);
        let t = topic.replace('*', &idx.next().expect("int").to_string());
        let _ = client
            .send_message(&t, payload.to_string().as_bytes(), QoS1, false)
            .await;
    }

    Timer::after(Duration::from_millis(DELAYMS)).await;
}

pub fn get_device<'a>() -> Device<'a> {
    DeviceBuilder::default()
        .identifiers(embassy_stm32::uid::uid_hex())
        .name(core::env!("MQTTCLIENTID")) //name of site
        .manufacturer("Rand")
        .model("Toucan BMS")
        .serial_number(&embassy_stm32::uid::uid_hex()[..12])
        .hw_version("JZ-STM32F407VET6")
        .sw_version(core::env!("CARGO_PKG_VERSION"))
        .configuration_url("https://rand12345.github.io")
        .build()
}

pub fn get_sensor(
    name: &str,
    unit: &str,
    template: &str,
    class: &str,
    device: &Device<'_>,
) -> Value {
    SensorBuilder::default()
        .device_class(class)
        .state_topic(BASETOPIC)
        .state_class("measurement")
        .unit_of_measurement(unit)
        .value_template(template)
        .name(name)
        .unique_id(&name.replace(' ', "_"))
        .device(*device)
        .build()
        .into()
}
