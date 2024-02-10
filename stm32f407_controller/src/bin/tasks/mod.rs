use defmt::{info, warn};
use embassy_stm32::{
    gpio::{Level, Output, OutputType, Speed},
    peripherals::{PA4, PA6, TIM3},
    timer::CountingMode,
};

use embassy_stm32::time::khz;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::timer::Channel;
use embassy_time::{Duration, Timer};

use crate::statics::CONTACTOR_STATE;

pub mod can_interfaces;

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "tcp_debug")]
pub mod tcp_debug;

#[cfg(feature = "ze40")]
pub mod can_processors_ze40;
#[cfg(feature = "ze40")]
pub use can_processors_ze40::{bms_rx, bms_tx_periodic};

#[cfg(feature = "ze50")]
pub mod can_processors_ze50;
#[cfg(feature = "ze50")]
pub use can_processors_ze50::{bms_rx, bms_tx_periodic};

#[cfg(feature = "tesla_m3")]
pub mod can_processors_tesla_m3;
#[cfg(feature = "tesla_m3")]
pub use can_processors_tesla_m3::{bms_rx, bms_tx_periodic};

#[cfg(any(feature = "foxess", feature = "solax"))]
pub mod can_processors_solax;
#[cfg(any(feature = "foxess", feature = "solax"))]
pub use can_processors_solax::inverter_rx;

#[cfg(any(feature = "pylontech", feature = "byd", feature = "goodwe"))]
pub mod can_processors_pylontech;
#[cfg(any(feature = "pylontech", feature = "byd", feature = "goodwe"))]
pub use can_processors_pylontech::inverter_rx;

#[cfg(feature = "forceh2")]
pub mod can_processors_pylontech_forceh2;
#[cfg(feature = "forceh2")]
pub use can_processors_pylontech_forceh2::inverter_rx;

pub mod leds;

#[cfg(any(feature = "modbus_bridge", feature = "modbus_client"))]
pub mod modbus;

#[cfg(feature = "mqtt")]
pub mod mqtt;

#[cfg(feature = "ntp")]
pub mod ntp;

// Misc tasks

#[embassy_executor::task]
pub async fn contactor_main_task(main: PA6, timer: TIM3) {
    let ch1 = PwmPin::new_ch1(main, OutputType::PushPull);
    let counting_mode = CountingMode::EdgeAlignedDown;
    let mut pwm = SimplePwm::new(timer, Some(ch1), None, None, None, khz(1), counting_mode);
    let max = pwm.get_max_duty() - 1;
    let mut active = false;
    loop {
        let state = CONTACTOR_STATE.wait().await;
        match (state, active) {
            (false, true) => {
                warn!("Contactor shutdown");
                pwm.disable(Channel::Ch1);
                info!("Contactor disabled");
                active = false;
                // LED_COMMAND.signal(crate::tasks::leds::LedCommand::Off(
                //     crate::tasks::leds::Led::Led3,
                // ))
            }
            (true, false) => {
                warn!("Activate 100% duty, wait 100ms, set duty to 50%, set active to true");
                pwm.enable(Channel::Ch1);
                info!("Contactor enabled");
                pwm.set_duty(Channel::Ch1, max);
                info!("Contactor at 100%");
                Timer::after(Duration::from_millis(100)).await;
                pwm.set_duty(Channel::Ch1, (max / 4) * 3);
                info!("Contactor at hold 50%");
                active = true;
                // LED_COMMAND.signal(crate::tasks::leds::LedCommand::On(
                //     crate::tasks::leds::Led::Led3,
                // ))
            }
            (true, true) => {
                info!("Contactor holding");
                // LED_COMMAND.signal(crate::tasks::leds::LedCommand::Blink(
                //     crate::tasks::leds::Led::Led3,
                // ))
            }
            _ => (),
        }
    }
}

#[embassy_executor::task]
pub async fn contactor_both_task(pre: PA4, main: PA6, timer: TIM3) {
    let mut pre = Output::new(pre, Level::Low, Speed::Medium);
    let main = PwmPin::new_ch1(main, OutputType::PushPull);

    let counting_mode = CountingMode::EdgeAlignedDown;
    let mut pwm = SimplePwm::new(timer, Some(main), None, None, None, khz(1), counting_mode);
    let max = pwm.get_max_duty() - 1;
    let mut active = false;
    loop {
        let state = CONTACTOR_STATE.wait().await;
        match (state, active) {
            (false, true) => {
                warn!("Contactors shutdown");
                pwm.disable(Channel::Ch1);
                pre.set_low();
                info!("Contactors disabled");
                active = false;
            }
            (true, false) => {
                warn!("Activate precharge contactor");
                pre.set_high();
                Timer::after(Duration::from_millis(500)).await;
                info!("Contactor main enabled");
                pwm.enable(Channel::Ch1);
                pwm.set_duty(Channel::Ch1, max);
                info!("Contactor main at 100%");
                Timer::after(Duration::from_millis(100)).await;
                pre.set_low();
                info!("Precharge disabled");
                pwm.set_duty(Channel::Ch1, (max / 4) * 3);
                info!("Contactor main PWM holding");
                active = true
            }
            (true, true) => info!("Contactor holding"),
            _ => (),
        }
    }
}
