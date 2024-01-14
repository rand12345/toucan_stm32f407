use crate::statics::*;
use bms_standard::Bms;
use defmt::warn;
use defmt::{error, info};
use embassy_stm32::can::bxcan;
use pylontech_force_h2_protocol::ForceH2;

#[allow(unused_assignments)]
#[cfg(feature = "forceh2")]
#[embassy_executor::task]
pub async fn inverter_rx() -> ! {
    warn!("Starting Force H2 Processor");
    let mut inverter = ForceH2::default();
    let mut inverter_comms_valid = false;
    let recv = INVERTER_CHANNEL_RX.receiver();
    let trans = INVERTER_CHANNEL_TX.sender();
    let canid = |frame: &bxcan::Frame| -> Option<u32> {
        match frame.id() {
            bxcan::Id::Standard(_) => None,
            bxcan::Id::Extended(id) => Some(id.as_raw()),
        }
    };
    loop {
        let frame = recv.receive().await;
        warn!("Debug: Inv >> STM {}", frame);
        if Some(0x4210) != canid(&frame) {
            continue;
        }

        inverter_comms_valid = false;

        if let Some(time) = *LAST_BMS_MESSAGE.lock().await {
            if time.elapsed().as_secs() > LAST_READING_TIMEOUT_SECS {
                error!("BMS last update timeout, inverter communications stopped");
                CONTACTOR_STATE.signal(inverter_comms_valid);
                continue;
            }
        }

        let bms: Bms = *BMS.lock().await;
        if !bms.valid {
            warn!("BMS data is not valid, skipping inverter send");
            continue;
        };

        match inverter.parser(&bms, frame) {
            Ok(iter) => {
                inverter_comms_valid = true;
                for frame in iter {
                    info!("Sending PylonTech H2 frame {:?}", frame);
                    trans.send(frame).await;
                }
            }
            Err(e) => warn!("Error parsing inverter frame: {:?}", e),
        };

        CONTACTOR_STATE.signal(inverter_comms_valid);
        #[cfg(feature = "mqtt")]
        SEND_MQTT.signal(true);
    }
}
