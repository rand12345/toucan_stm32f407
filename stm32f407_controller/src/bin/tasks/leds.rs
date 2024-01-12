use embassy_stm32::gpio::{AnyPin, Output};
use embassy_time::{Duration, Timer};

use crate::statics::LED_COMMAND;

#[embassy_executor::task]
pub async fn led_task(leds: Leds) {
    defmt::info!("Spawn activity LEDs");
    let mut leds = leds;
    loop {
        // change on static state
        let command = LED_COMMAND.wait().await;
        leds.process_command(command).await;
    }
}
#[allow(dead_code)]
pub enum LedCommand {
    On(Led),
    Off(Led),
    Blink(Led),
    Toggle(Led),
}

#[allow(dead_code)]
pub enum Led {
    Led1,
    Led2,
    Led3,
}
type LedType = Output<'static, AnyPin>;
pub struct Leds(LedType, LedType, LedType);

impl Leds {
    pub fn new(
        led1: Output<'static, AnyPin>,
        led2: Output<'static, AnyPin>,
        led3: Output<'static, AnyPin>,
    ) -> Leds {
        Leds(led1, led2, led3)
    }
    fn get_led(&mut self, l: Led) -> &mut LedType {
        match l {
            Led::Led1 => &mut self.0,
            Led::Led2 => &mut self.1,
            Led::Led3 => &mut self.2,
        }
    }
    pub async fn process_command(&mut self, lc: LedCommand) {
        match lc {
            LedCommand::On(l) => self.on(l),
            LedCommand::Off(l) => self.off(l),
            LedCommand::Blink(l) => self.blink(l).await,
            LedCommand::Toggle(l) => self.toggle(l),
        };
    }
    fn on(&mut self, l: Led) {
        self.get_led(l).set_low()
    }
    fn off(&mut self, l: Led) {
        self.get_led(l).set_high()
    }
    async fn blink(&mut self, l: Led) {
        let led = self.get_led(l);
        if led.is_set_low() {
            led.set_high();
            Timer::after(Duration::from_millis(100)).await;
        }
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
        led.set_high();
    }
    fn toggle(&mut self, l: Led) {
        let led = self.get_led(l);
        led.toggle();
    }
}
