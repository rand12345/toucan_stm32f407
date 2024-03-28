use crate::statics::*;
use defmt::{error, info};
use defmt::{warn, Debug2Format};
use embassy_stm32::can::bxcan::Frame;
const INVERTER_SEND_MS: u64 = 1000;

#[cfg(feature = "byd")]
use byd_protocol as Inverter;

#[cfg(feature = "pylontech")]
const LABEL: &str = "PylonTech";

#[cfg(feature = "byd")]
const LABEL: &str = "BYD";

#[cfg(feature = "pylontech")]
use pylontech_protocol as Inverter;

#[allow(unused_assignments)]
#[cfg(any(feature = "pylontech", feature = "byd"))]
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
        Timer::after(Duration::from_millis(INVERTER_SEND_MS * 2)).await;
        inverter_comms_valid = false;

        if let Some(time) = *LAST_BMS_MESSAGE.lock().await {
            if time.elapsed().as_secs() > LAST_READING_TIMEOUT_SECS {
                error!("BMS last update timeout, inverter communications stopped");
                CONTACTOR_STATE.signal(inverter_comms_valid);
                continue;
            }
        }

        /*
         *
         *
         * 6.009796 DEBUG Bms { config: Config { charge_current: MinMax { min: 0.0, max: 250.0 }, discharge_current: MinMax { min: 0.0, max: 250.0 }, current_sensor: MinMax { min: -200.0, max: 200.0 }, pack_volts: MinMax { min: 300.0, max: 400.0 }, cell_temperatures: MinMax { min: -20.0, max: 50.0 }, pack_temperatures: MinMax { min: -20.0, max: 50.0 }, cell_millivolt_peak: 4200, cells_mv: MinMax { min: 3000, max: 4150 }, cell_millivolt_delta_max: 500, soc: MinMax { min: 0, max: 100 } }, cell_mv: CellsMv([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
         * temps: MinMax { min: 9.0, max: 9.0 },
         * charge_max: 250.0, discharge_max: 0.0,
         * bal_cells: [false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false],
         * cell_delta_mv: 28, cell_range_mv: MinMax { min: 3698, max: 3726 },
         * pack_volts: 347.80002, current: 0.09375, kwh_remaining: 5.7000003,
         * soc: 0.0, soh: 0, temp: 9.0, valid: true, dod: MinMax { min: 5, max: 90 } }
         */
        {
            let bms = *BMS.lock().await;
            defmt::debug!("{:?}", bms);
            if !bms.valid {
                warn!("BMS data is not valid, skipping inverter send");
                continue;
            };
            // drops mutex
            for frame in Inverter::iter::<Frame>(bms) {
                Timer::after(Duration::from_millis(50)).await;
                info!("Sending PylonTech frame {:?}", frame.data());
                if let Err(e) = trans.try_send(frame) {
                    error!("Inv send {}", e)
                };
            }
        }
        CONTACTOR_STATE.signal(inverter_comms_valid);
        #[cfg(feature = "mqtt")]
        SEND_MQTT.signal(true);
    }
}
