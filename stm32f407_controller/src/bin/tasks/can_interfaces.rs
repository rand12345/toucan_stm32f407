use crate::{
    statics::*,
    tasks::leds::{
        Led::{Led1, Led2},
        LedCommand::Toggle,
    },
    types::FRAME_BUFFER,
};
use defmt::warn;
use embassy_futures::select::{select, Either};
use embassy_stm32::{
    can::{
        bxcan::{self, *},
        Can, CanRx, CanTx,
    },
    peripherals::*,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Receiver, Sender},
};
use embassy_time::Duration;

#[embassy_executor::task]
pub async fn inverter_task(mut can: Can<'static, CAN2>, baud: u32) {
    let inv_rx = INVERTER_CHANNEL_RX.sender();
    let inv_tx = INVERTER_CHANNEL_TX.receiver();
    // Wait for CAN1 to initalise
    CAN_READY.wait().await;
    can2_init(&can).await;
    can.set_bitrate(baud);
    can.enable().await;

    warn!("Starting Inverter Can2");
    let (mut tx, mut rx) = can.split();
    loop {
        LED_COMMAND.signal(Toggle(Led2));
        can_routine::<CAN2, FRAME_BUFFER>(&mut rx, &mut tx, inv_rx, inv_tx).await;
    }
}

#[embassy_executor::task]
pub async fn bms_task(mut can: Can<'static, CAN1>, baud: u32) {
    let bms_rx = BMS_CHANNEL_RX.sender();
    let bms_tx = BMS_CHANNEL_TX.receiver();
    can1_init(&can).await;
    can.set_bitrate(baud);
    can.enable().await;
    warn!("Starting BMS Can1");
    // Signal to CAN2 that filters have been applied
    CAN_READY.signal(true);
    let (mut tx, mut rx) = can.split();
    loop {
        LED_COMMAND.signal(Toggle(Led1));
        can_routine::<CAN1, FRAME_BUFFER>(&mut rx, &mut tx, bms_rx, bms_tx).await;
    }
}

#[inline]
async fn can_routine<C, const B: usize>(
    rx: &mut CanRx<'_, '_, C>,
    tx: &mut CanTx<'_, '_, C>,
    ch_rx: Sender<'_, CriticalSectionRawMutex, bxcan::Frame, B>,
    ch_tx: Receiver<'_, CriticalSectionRawMutex, bxcan::Frame, B>,
) where
    C: embassy_stm32::can::Instance,
{
    let name = core::any::type_name::<C>();
    match select(rx.wait_not_empty(), ch_tx.receive()).await {
        Either::First(_) => {
            match rx.read().await {
                Ok(envelope) => ch_rx.send(envelope.frame).await,
                Err(e) => {
                    defmt::error!("{} {}", name, e);
                    embassy_time::Timer::after(Duration::from_millis(50)).await;
                }
            };
        }
        Either::Second(frame) => {
            tx.write(&frame).await;
        }
    }
}

async fn can1_init(can: &Can<'static, CAN1>) {
    // BMS Filter ============================================
    #[cfg(feature = "ze50")]
    can.as_mut().modify_filters().set_split(1).enable_bank(
        0,
        Fifo::Fifo1,
        filter::Mask32::frames_with_ext_id(
            ExtendedId::new(0x18DAF1DB).unwrap(),
            ExtendedId::new(0x1ffffff).unwrap(),
        ),
    );

    #[cfg(not(feature = "ze50"))]
    can.as_mut().modify_filters().set_split(1).enable_bank(
        0,
        Fifo::Fifo1,
        filter::Mask32::accept_all(),
    );

    // Inverter Filter ============================================
    can.as_mut().modify_filters().slave_filters().enable_bank(
        1,
        Fifo::Fifo0,
        filter::Mask32::accept_all(),
    );

    can.as_mut()
        .modify_config()
        .set_loopback(false) // Receive own frames
        .set_silent(false)
        .leave_disabled();
}

async fn can2_init(can: &Can<'static, CAN2>) {
    can.as_mut()
        .modify_config()
        .set_loopback(false) // Receive own frames
        .set_silent(false)
        .leave_disabled();
}
