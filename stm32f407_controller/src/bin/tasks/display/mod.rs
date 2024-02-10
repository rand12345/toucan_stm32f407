use crate::statics::BMS;
use crate::types::{Spi2Display, Spi2Interface};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_stm32::{
    gpio::{Level, Output, Pin, Speed},
    peripherals::{PE7, PE8, PE9},
    spi,
};
use embassy_time::{Delay, Duration, Ticker};

use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::*,
    text::*,
};
use st7735_embassy::ST7735;
use static_cell::StaticCell;

#[embassy_executor::task]
pub async fn display_task(spi: Spi2Interface<'static>, dc: PE8, d_cs: PE9, rst: PE7) {
    use core::fmt::Write;
    use Alignment::*;
    let dc = Output::new(dc.degrade(), Level::High, Speed::VeryHigh);
    let display_cs = Output::new(d_cs.degrade(), Level::High, Speed::VeryHigh);
    let rst = Output::new(rst.degrade(), Level::High, Speed::VeryHigh);

    let mut display_config = spi::Config::default();
    display_config.frequency = embassy_stm32::time::Hertz(36_000_000);
    static SPI_BUS: StaticCell<Spi2Display> = StaticCell::new();
    let spi_bus = embassy_sync::mutex::Mutex::new(spi);
    let spi_bus = SPI_BUS.init(spi_bus);
    let spi_display = SpiDeviceWithConfig::new(spi_bus, display_cs, display_config);
    let mut display = ST7735::new(spi_display, dc, rst, Default::default(), 160, 131);
    display
        .init(&mut Delay)
        .await
        .expect("Display failed to initialise");
    display.clear(Rgb565::BLACK).expect("Clear fail");

    let bounding_box = display.bounding_box();
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_8X13)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    let mut s = heapless::String::<20>::new();
    let mut ticker = Ticker::every(Duration::from_secs(5));
    loop {
        let mut format_line = |s: &str, l: i32, j: Alignment| {
            let alignment = TextStyleBuilder::new()
                .alignment(j)
                .baseline(Baseline::Top)
                .build();

            let x: i32 = match j {
                Left => 1,
                Center => 80,
                Right => 160,
            };
            Text::with_text_style(
                s,
                bounding_box.top_left + Point::new(x, l * 14),
                character_style,
                alignment,
            )
            .draw(&mut display)
            .unwrap();
        };
        ticker.next().await;
        let data: DisplayFormat = (*BMS.lock().await).into();

        let lines: [Line; 9] = [
            Line::new("---- BMS ----", 0.0, ""),
            Line::new("SoC", data.soc, "%"),
            Line::new("Remaining ", data.kwh, "kWh"),
            Line::new("Voltage", data.volts, "V"),
            Line::new("Cell Temp", data.cell_temp_high, "oC"),
            Line::new("Cell V High", data.cell_mv_high.into(), "mV"),
            Line::new("Cell V Low", data.cell_mv_low.into(), "mV"),
            Line::new("Current ", data.amps, "A"),
            Line::new("Balancing ", data.bal as f32, "#"),
        ];
        format_line(lines[0].key, 0, Center);
        for (i, line) in lines.iter().enumerate().skip(1) {
            s.clear();
            write!(&mut s, "{:.1}{}", line.value, line.unit).unwrap();
            format_line(line.key, i as i32, Left);
            format_line(&s, i as i32, Right);
        }
        display.flush().await.unwrap();
    }
}

struct Line<'a> {
    pub key: &'a str,
    pub value: f32,
    pub unit: &'a str,
}
impl<'a> Line<'a> {
    fn new(key: &'a str, value: f32, unit: &'a str) -> Line<'a> {
        Line { key, value, unit }
    }
}
#[derive(Clone, Copy)]
pub struct DisplayFormat {
    soc: f32,
    volts: f32,
    cell_mv_high: u16,
    cell_mv_low: u16,
    cell_temp_high: f32,
    _cell_temp_low: f32,
    amps: f32,
    kwh: f32,
    _charge: f32,
    _discharge: f32,
    bal: u8,
    _valid: bool,
}

impl From<bms_standard::Bms> for DisplayFormat {
    fn from(bmsdata: bms_standard::Bms) -> Self {
        DisplayFormat {
            soc: bmsdata.soc,
            volts: bmsdata.pack_volts,
            cell_mv_high: *bmsdata.cell_range_mv.maximum(),
            cell_mv_low: *bmsdata.cell_range_mv.minimum(),
            cell_temp_high: *bmsdata.temps.maximum(),
            _cell_temp_low: *bmsdata.temps.minimum(),
            // cells_millivolts : bmsdata.cells;
            // cell_balance  bmsdata.bal_cells;
            amps: bmsdata.current,
            kwh: bmsdata.kwh_remaining,
            _charge: bmsdata.charge_max,
            _discharge: bmsdata.discharge_max,
            bal: bmsdata.get_balancing_cells(),
            _valid: bmsdata.valid,
        }
    }
}
