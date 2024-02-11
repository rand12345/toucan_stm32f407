#![no_std]
#![no_main]
use serde_json::json;
pub use serde_json::Value;

#[derive(Debug, Copy, Clone)]
pub struct Device<'a> {
    identifiers: [&'a str; 1],
    name: &'a str,
    manufacturer: &'a str,
    model: &'a str,
    serial_number: &'a str,
    hw_version: &'a str,
    sw_version: &'a str,
    configuration_url: &'a str,
}

#[derive(Debug, Clone)]
pub struct Sensor<'a> {
    device_class: &'a str,
    state_topic: &'a str,
    unit_of_measurement: &'a str,
    value_template: &'a str,
    state_class: &'a str,
    unique_id: &'a str,
    name: &'a str,
    device: Device<'a>,
}
impl<'a> From<Sensor<'a>> for Value {
    fn from(sensor: Sensor) -> Self {
        json!({
            "device_class": sensor.device_class,
            "state_topic": sensor.state_topic,
            "unit_of_measurement": sensor.unit_of_measurement,
            "value_template": sensor.value_template,
            "state_class": sensor.state_class,
            "unique_id": sensor.unique_id,
            "name": sensor.name,
            "device": {
                "identifiers": sensor.device.identifiers,
                "name": sensor.device.name,
                "manufacturer": sensor.device.manufacturer,
                "model": sensor.device.model,
                "serial_number": sensor.device.serial_number,
                "hw_version": sensor.device.hw_version,
                "sw_version": sensor.device.sw_version,
                "configuration_url": sensor.device.configuration_url,
            },
        })
    }
}

#[derive(Default)]
pub struct SensorBuilder<'a> {
    device_class: Option<&'a str>,
    state_topic: Option<&'a str>,
    unit_of_measurement: Option<&'a str>,
    value_template: Option<&'a str>,
    state_class: Option<&'a str>,
    name: Option<&'a str>,
    unique_id: Option<&'a str>,
    device: Option<Device<'a>>,
}

impl<'a> SensorBuilder<'a> {
    pub fn device_class(mut self, device_class: &'a str) -> Self {
        self.device_class = Some(device_class);
        self
    }
    pub fn unique_id(mut self, unique_id: &'a str) -> Self {
        self.unique_id = Some(unique_id);
        self
    }

    pub fn state_topic(mut self, state_topic: &'a str) -> Self {
        self.state_topic = Some(state_topic);
        self
    }

    pub fn unit_of_measurement(mut self, unit_of_measurement: &'a str) -> Self {
        self.unit_of_measurement = Some(unit_of_measurement);
        self
    }

    pub fn value_template(mut self, value_template: &'a str) -> Self {
        self.value_template = Some(value_template);
        self
    }

    pub fn name(mut self, unique_name: &'a str) -> Self {
        self.name = Some(unique_name);
        self
    }

    pub fn state_class(mut self, state_class: &'a str) -> Self {
        self.state_class = Some(state_class);
        self
    }

    pub fn device(mut self, device: Device<'a>) -> Self {
        self.device = Some(device);
        self
    }

    pub fn build(self) -> Sensor<'a> {
        Sensor {
            device_class: self.device_class.expect("device_class is required"),
            state_topic: self.state_topic.expect("state_topic is required"),
            unit_of_measurement: self
                .unit_of_measurement
                .expect("unit_of_measurement is required"),
            unique_id: self.unique_id.expect("unique_id is required"),
            value_template: self.value_template.expect("value_template is required"),
            name: self.name.expect("unique_name is required"),
            device: self.device.expect("device is required"),
            state_class: self.state_class.expect("state_class is required"),
        }
    }
}
#[derive(Default)]
pub struct DeviceBuilder<'a> {
    identifiers: [&'a str; 1],
    name: Option<&'a str>,
    manufacturer: Option<&'a str>,
    model: Option<&'a str>,
    serial_number: Option<&'a str>,
    hw_version: Option<&'a str>,
    sw_version: Option<&'a str>,
    configuration_url: Option<&'a str>,
}

impl<'a> DeviceBuilder<'a> {
    pub fn identifiers(mut self, identifiers: &'a str) -> Self {
        self.identifiers = [identifiers];
        self
    }

    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    pub fn manufacturer(mut self, manufacturer: &'a str) -> Self {
        self.manufacturer = Some(manufacturer);
        self
    }

    pub fn model(mut self, model: &'a str) -> Self {
        self.model = Some(model);
        self
    }

    pub fn serial_number(mut self, serial_number: &'a str) -> Self {
        self.serial_number = Some(serial_number);
        self
    }

    pub fn hw_version(mut self, hw_version: &'a str) -> Self {
        self.hw_version = Some(hw_version);
        self
    }

    pub fn sw_version(mut self, sw_version: &'a str) -> Self {
        self.sw_version = Some(sw_version);
        self
    }

    pub fn configuration_url(mut self, configuration_url: &'a str) -> Self {
        self.configuration_url = Some(configuration_url);
        self
    }

    pub fn build(self) -> Device<'a> {
        Device {
            identifiers: self.identifiers,
            name: self.name.expect("name is required"),
            manufacturer: self.manufacturer.expect("manufacturer is required"),
            model: self.model.expect("model is required"),
            serial_number: self.serial_number.expect("serial_number is required"),
            hw_version: self.hw_version.expect("hw_version is required"),
            sw_version: self.sw_version.expect("sw_version is required"),
            configuration_url: self
                .configuration_url
                .expect("configuration_url is required"),
        }
    }
}

/*
fn main() {
    let sensor = Sensor::builder()
        .device_class("temperature")
        .state_topic("homeassistant/sensor/sensorBedroom/state")
        .unit_of_measurement("°C")
        .value_template("{{ value_json.temperature }}")
        .unique_id("temp01ae")
        .device(
            DeviceBuilder::default()
                .identifiers("bedroom01ae")
                .name("Bedroom")
                .manufacturer("Example sensors Ltd.")
                .model("K9")
                .serial_number("12AE3010545")
                .hw_version("1.01a")
                .sw_version("2024.1.0")
                .configuration_url("https://example.com/sensor_portal/config")
                .build(),
        )
        .build();
}

*/
