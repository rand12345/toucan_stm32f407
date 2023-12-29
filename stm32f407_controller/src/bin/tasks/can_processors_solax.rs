use crate::statics::*;

use defmt::{error, info, warn};
use embassy_stm32::can::bxcan::{Frame, Id::*};

#[cfg(feature = "foxess")]
use foxess_protocol::{FoxEssBms as Inverter, FoxEssError as InverterError};
#[cfg(feature = "foxess")]
const LABEL: &str = "FoxESS";

#[cfg(feature = "solax")]
const LABEL: &str = "Solax";
#[cfg(feature = "solax")]
use solax_protocol::{SolaxBms as Inverter, SolaxError as InverterError};

#[allow(unused_assignments)]
#[embassy_executor::task]
pub async fn inverter_rx() -> ! {
    warn!("Starting {} Inverter Process", LABEL);

    let recv = INVERTER_CHANNEL_RX.receiver();
    let sender = INVERTER_CHANNEL_TX.sender();

    let mut inverter = Inverter::default();
    let mut initalised = false;
    loop {
        let frame: Frame = recv.receive().await;
        if let Extended(id) = frame.id() {
            if id.as_raw() != 0x1871 {
                continue;
            }
        }

        match *LAST_BMS_MESSAGE.lock().await {
            Some(time) => {
                if time.elapsed().as_secs() > LAST_READING_TIMEOUT_SECS {
                    error!("BMS last update timeout, inverter communications stopped");
                    #[cfg(not(feature = "tesla_m3"))]
                    CONTACTOR_STATE.signal(false);
                    continue;
                }
            }
            None => {
                warn!("Inverter request ignored, BMS not yet seen");
                #[cfg(not(feature = "tesla_m3"))]
                CONTACTOR_STATE.signal(false);
                continue;
            }
        };

        let response = {
            let bms = BMS.lock().await;
            if !bms.valid {
                warn!("Inverter request ignored, BMS data not yet valid");
            };
            inverter.parser(frame, &bms, true)
        };

        let inverter_comms_valid = match response {
            Ok(frames) => {
                info!("Sending to {} inverter", LABEL);
                for frame in frames {
                    sender.send(frame).await;
                }
                SEND_MQTT.signal(true);
                true
            }
            Err(e) => {
                use InverterError::*;
                match e {
                    InvalidFrameEncode(id) => {
                        error!("Critical: frame encoding failed for {:02x}", id);
                        false // disable contactor
                    }
                    BadId(id) => {
                        error!("Critical: unexpected frame in inverter can data {:02x}", id);
                        false // disable contactor
                    }
                    TimeStamp(time) => {
                        info!(
                            "Inverter Time: 20{}-{}-{} {}:{}:{}",
                            time[0], time[1], time[2], time[3], time[4], time[5]
                        );
                        continue;
                    }
                    UnwantedFrame => continue,
                    x => {
                        warn!("Solax error: {}", x);
                        true
                    }
                }
            }
        };
        if !initalised {
            let mut gs = GLOBALSTATE.lock().await;
            match inverter_comms_valid {
                true => gs.set_fault(crate::config::Fault::None),
                false => gs.set_fault(crate::config::Fault::InvFault),
            }
        }
        #[cfg(not(feature = "tesla_m3"))]
        CONTACTOR_STATE.signal(inverter_comms_valid && initalised);
        // waits for 2 positive results before activating contactor
        initalised = inverter_comms_valid
    }
}
