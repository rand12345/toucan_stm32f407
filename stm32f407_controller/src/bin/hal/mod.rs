#![allow(unused_imports)]
use embassy_net::Stack;
use embassy_stm32::{
    bind_interrupts,
    can::{Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler},
    eth::{self, generic_smi::GenericSMI, Ethernet, PacketQueue},
    peripherals::*,
    sdmmc::Sdmmc,
    spi::Spi,
    time::mhz,
    usart,
};

use static_cell::make_static;

use crate::{
    statics,
    types::{EthDevice, Usart6Type, RS232, RS485},
    MAC_ADDR,
};

// bind_interrupts!(struct IrqI2c {
//     I2C1_EV => embassy_stm32::i2c::InterruptHandler<embassy_stm32::peripherals::I2C1>;
// });

#[embassy_executor::task]
pub async fn net_task(stack: &'static Stack<EthDevice>) -> ! {
    stack.run().await
}

pub fn peripherals_config() -> embassy_stm32::Config {
    let mut config = embassy_stm32::Config::default();
    config.rcc.hse = Some(mhz(25));
    config.rcc.sys_ck = Some(mhz(168));
    config.rcc.hclk = Some(mhz(148));
    config.rcc.pclk1 = Some(mhz(48));
    config.rcc.pclk2 = Some(mhz(84));
    config.rcc.pll48 = true;
    config
}

#[cfg(feature = "nvs")]
pub fn spi2(p: &Peripherals) -> Spi<'static, SPI2, DMA1_CH0, DMA1_CH1> {
    let mut config = embassy_stm32::spi::Config::default();
    config.frequency = 40_000_000;
    // config.mode = embassy_stm32::spi::Mode {
    //     polarity: embassy_stm32::spi::Polarity::IdleHigh,
    //     phase: embassy_stm32::spi::Phase::CaptureOnSecondTransition,
    // };

    Spi::new(
        p.SPI2, p.PB10, //clk
        p.PC3,  // mosi
        p.PC2,  //miso
        p.DMA1_CH0, p.DMA1_CH1, config,
    )
}

#[cfg(feature = "spi")]
use embassy_stm32::dma::NoDma;
#[cfg(feature = "spi")]
pub fn spi3(p: SPI3, sck: PB3, mosi: PC12, miso: PB4) -> Spi<'static, SPI3, NoDma, NoDma> {
    let mut config = embassy_stm32::spi::Config::default();
    config.frequency = Hertz(1_000_000);
    Spi::new(p, sck, mosi, miso, NoDma, NoDma, config)
}

pub fn rs485(
    p: USART2,
    rx_pin: PD6,
    tx_pin: PD5,
    tx_dma: DMA1_CH6,
    rx_dma: DMA1_CH5,
) -> RS485<'static> {
    bind_interrupts!(struct IrqUSART2 {
        USART2 => embassy_stm32::usart::InterruptHandler<embassy_stm32::peripherals::USART2>;
    });
    let mut config = embassy_stm32::usart::Config::default();
    config.baudrate = 9600;
    usart::Uart::new(p, rx_pin, tx_pin, IrqUSART2, tx_dma, rx_dma, config).unwrap()
}

#[cfg(feature = "rs232")]
pub fn rs232(p: USART1, rx: PA10, tx: PA9, tx_dma: DMA2_CH0, rx_dma: DMA2_CH1) -> RS232<'static> {
    bind_interrupts!(struct IrqsUSART1 {
        USART1 => embassy_stm32::usart::InterruptHandler<embassy_stm32::peripherals::USART1>;
    });
    let mut config = usart::Config::default();
    config.baudrate = 115200;
    usart::Uart::new(p, rx, tx, IrqsUSART1, tx_dma, rx_dma, config).unwrap()
}

#[cfg(feature = "usart6")]
pub fn usart6(
    p: USART6,
    rx: PC7,
    tx: PC6,
    tx_dma: DMA2_CH7,
    rx_dma: DMA2_CH2,
) -> Usart6Type<'static> {
    // Top 16-pin header
    bind_interrupts!(struct IrqUSART6 {
        USART6 => embassy_stm32::usart::InterruptHandler<embassy_stm32::peripherals::USART6>;
    });
    let mut config = usart::Config::default();
    config.baudrate = 115200;
    usart::Uart::new(p, rx, tx, IrqUSART6, tx_dma, rx_dma, config).unwrap()
}

#[cfg(feature = "sdmmc")]
pub fn sdmmc(
    p: SDIO,
    dma: DMA2_CH6,
    clk: PC12,
    cmd: PD2,
    d0: PC8,
    d1: PC9,
    d2: PC10,
    d3: PC11,
) -> embassy_stm32::sdmmc::Sdmmc<'static, embassy_stm32::peripherals::SDIO, DMA2_CH6> {
    bind_interrupts!(struct IrqSDIO {
        SDIO => embassy_stm32::sdmmc::InterruptHandler<embassy_stm32::peripherals::SDIO>;
    });
    Sdmmc::new_4bit(
        p,
        IrqSDIO,
        dma,
        clk,
        cmd,
        d0,
        d1,
        d2,
        d3,
        Default::default(),
    )
}

bind_interrupts!(struct IrqCAN {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
    CAN2_RX0 => Rx0InterruptHandler<CAN2>;
    CAN2_RX1 => Rx1InterruptHandler<CAN2>;
    CAN2_SCE => SceInterruptHandler<CAN2>;
    CAN2_TX => TxInterruptHandler<CAN2>;
});

pub fn can1(p: CAN1, rx: PD0, tx: PD1) -> Can<'static, CAN1> {
    Can::new(p, rx, tx, IrqCAN)
}
pub fn can2(p: CAN2, rx: PB5, tx: PB6) -> Can<'static, CAN2> {
    Can::new(p, rx, tx, IrqCAN)
}

pub async fn get_eth(
    p: ETH,
    ref_clk: PA1,
    mdio: PA2,
    mdc: PC1,
    crs: PA7,
    rx_d0: PC4,
    rx_d1: PC5,
    tx_d0: PB12,
    tx_d1: PB13,
    tx_en: PB11,
) -> (
    eth::Ethernet<'static, ETH, GenericSMI>,
    embassy_net::Config,
    u64,
) {
    // Ethernet
    bind_interrupts!(struct IrqETH {
        ETH => eth::InterruptHandler;
    });
    // Generate random seed.

    // let mut rng = Rng::new(p.RNG, IrqETH);
    let seed = [0; 8];
    // let _ = rng.async_fill_bytes(&mut seed).await;
    let seed = u64::from_le_bytes(seed);

    // let seed = u64::from_le_bytes(seed);
    let eth = Ethernet::new(
        make_static!(PacketQueue::<4, 4>::new()),
        p,
        IrqETH,
        ref_clk,
        mdio,
        mdc,
        crs,
        rx_d0,
        rx_d1,
        tx_d0,
        tx_d1,
        tx_en, //tx_en
        GenericSMI::new(),
        MAC_ADDR,
        1,
    );
    let netconfig = { statics::NETCONFIG.lock().await.get_config() };

    // Init network stack
    (eth, netconfig, seed)
}
