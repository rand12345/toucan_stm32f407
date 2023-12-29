use embassy_stm32::peripherals::IWDG;
use embassy_stm32::wdg::IndependentWatchdog;

use crate::statics::WDT;

#[embassy_executor::task]
pub async fn init(instance: IWDG, timeout: u32) {
    let mut wdt = IndependentWatchdog::new(instance, timeout); // 1sec

    wdt.unleash();

    loop {
        // await a signal and pet the dog, timeout triggers device reset
        let signal = WDT.wait().await;
        if signal {
            wdt.pet();
        }
    }
}
