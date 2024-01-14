use crate::statics::*;
use defmt::{error, info};
use defmt::{warn, Debug2Format};
use embassy_stm32::can::bxcan::Frame;
const INVERTER_SEND_MS: u64 = 1000;

#[cfg(feature = "byd")]
use byd_protocol as Inverter;

#[cfg(feature = "goodwe")]
use goodwe_protocol as Inverter;

#[cfg(feature = "pylontech")]
const LABEL: &str = "PylonTech";

#[cfg(feature = "byd")]
const LABEL: &str = "BYD";

#[cfg(feature = "goodwe")]
const LABEL: &str = "GoodWe";

#[cfg(feature = "pylontech")]
use pylontech_protocol as Inverter;

#[allow(unused_assignments)]
#[cfg(any(feature = "pylontech", feature = "byd", feature = "goodwe"))]
#[embassy_executor::task]
pub async fn inverter_rx() -> ! {
    use embassy_time::{Duration, Timer};
    warn!("Starting {} Inverter Processor", LABEL);
    let mut inverter_comms_valid = false;
    let recv = INVERTER_CHANNEL_RX.receiver();
    let trans = INVERTER_CHANNEL_TX.sender();
    loop {
        if let Ok(frame) = recv.try_receive() {
            warn!("Debug: Inv >> STM {}", Debug2Format(&frame))
        };
        Timer::after(Duration::from_millis(INVERTER_SEND_MS)).await;
        inverter_comms_valid = false;

        if let Some(time) = *LAST_BMS_MESSAGE.lock().await {
            if time.elapsed().as_secs() > LAST_READING_TIMEOUT_SECS {
                error!("BMS last update timeout, inverter communications stopped");
                CONTACTOR_STATE.signal(inverter_comms_valid);
                continue;
            }
        }

        let bms = *BMS.lock().await;
        if !bms.valid {
            warn!("BMS data is not valid, skipping inverter send");
            continue;
        };
        // drops mutex
        for frame in Inverter::iter::<Frame>(bms) {
            info!("Sending {} frame {:?}", LABEL, frame.data());
            trans.send(frame).await;
        }
        CONTACTOR_STATE.signal(inverter_comms_valid);
        #[cfg(feature = "mqtt")]
        SEND_MQTT.signal(true);
    }
}
