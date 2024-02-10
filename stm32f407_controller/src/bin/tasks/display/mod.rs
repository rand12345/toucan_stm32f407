use crate::statics::{BMS, MQTTCONFIG};
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
    let mut character_style = MonoTextStyleBuilder::new()
        .font(&FONT_8X13)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    let mut s = heapless::String::<20>::new();
    let ticker = Ticker::every(Duration::from_secs(5));
    loop {
        let mut format_line = |s: &str, l: i32, j: Alignment| {
            let alignment = TextStyleBuilder::new()
                .alignment(j)
                .baseline(Baseline::Top)
                .build();
            Text::with_text_style(
                s,
                bounding_box.top_left + Point::new(0, l * 14),
                character_style,
                alignment,
            )
            .draw(&mut display)
            .unwrap();
        };
        ticker.next().await;
        let data: DisplayFormat = (*BMS.lock().await).into();

        // Background red if data not valid
        if !data.valid {
            character_style = MonoTextStyleBuilder::new()
                .font(&FONT_8X13)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::RED)
                .build();
        } else if character_style.background_color == Some(Rgb565::RED) {
            character_style = MonoTextStyleBuilder::new()
                .font(&FONT_8X13)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::BLACK)
                .build();
        }
        let lines: [[&str; 2]; 8] = [
            ["SoC ", write!(s.clone(), "{:.1}%", data.soc)],
            ["Remaining ", write!(s.clone(), "{:.1}kWh", data.kwh)],
            ["Voltage", write!(s, "{:.1}V", data.volts)],
            [
                "Cell Temps ",
                write!(s, "{:.1}/{:.1}°C", data.cell_temp_high, data.cell_temp_low),
            ],
            ["Cell V High ", write!(s, "{}mV", data.cell_mv_high)],
            ["Cell V Low ", write!(s, "{}mV", data.cell_mv_low)],
            ["Current ", write!(s, "{:.1}A", data.amps)],
            ["Balancing ", write!(s, "{}", data.bal)],
        ];
        for (i, line) in lines.iter().enumerate() {
            format_line(line[0], i as i32, Left);
            format_line(line[1], i as i32, Right);
        }
        /*
        format_line("SoC:", 0, Left); //9x20
        s.clear();
        let _ = write!(s, "{:.1}%", data.soc);
        format_line(&s, 0, Right);
        format_line("Remaining", 1, Left);
        s.clear();
        let _ = write!(s, "{:.1}kWh", data.kwh);
        format_line(&s, 1, Right);
        format_line("Voltage", 2, Left);
        s.clear();
        let _ = write!(s, "{:.1}V", data.volts);
        format_line(&s, 2, Right);
        format_line("Cell Temps ", 3, Left);
        s.clear();
        let _ = write!(s, "{:.1}/{:.1}°C", data.cell_temp_high, data.cell_temp_low);
        format_line(&s, 3, Right);
        format_line("Cell V High ", 4, Left);
        s.clear();
        let _ = write!(s, "{}mV", data.cell_mv_high);
        format_line(&s, 4, Right);
        format_line("Cell V Low ", 5, Left);
        s.clear();
        let _ = write!(s, "{}mV", data.cell_mv_low);
        format_line(&s, 5, Right);
        format_line("Current ", 6, Left);
        s.clear();
        let _ = write!(s, "{:.1}A", data.amps);
        format_line(&s, 6, Right);
        format_line("Balancing ", 7, Left);
        s.clear();
        let _ = write!(s, "{}", data.bal);
        format_line(&s, 7, Right);
         */
        display.flush().await.unwrap();
    }
}

#[derive(Clone, Copy)]
pub struct DisplayFormat {
    soc: f32,
    volts: f32,
    cell_mv_high: u16,
    cell_mv_low: u16,
    cell_temp_high: f32,
    cell_temp_low: f32,
    amps: f32,
    kwh: f32,
    charge: f32,
    discharge: f32,
    bal: u8,
    valid: bool,
}

impl From<bms_standard::Bms> for DisplayFormat {
    fn from(bmsdata: bms_standard::Bms) -> Self {
        DisplayFormat {
            soc: bmsdata.soc,
            volts: bmsdata.pack_volts,
            cell_mv_high: *bmsdata.cell_range_mv.maximum(),
            cell_mv_low: *bmsdata.cell_range_mv.minimum(),
            cell_temp_high: *bmsdata.temps.maximum(),
            cell_temp_low: *bmsdata.temps.minimum(),
            // cells_millivolts : bmsdata.cells;
            // cell_balance  bmsdata.bal_cells;
            amps: bmsdata.current,
            kwh: bmsdata.kwh_remaining,
            charge: bmsdata.charge_max,
            discharge: bmsdata.discharge_max,
            bal: bmsdata.get_balancing_cells(),
            valid: bmsdata.valid,
        }
    }
}

impl DisplayFormat {
    pub fn default() -> Self {
        Self {
            soc: 0.0,
            volts: 0.0,
            cell_mv_high: 0,
            cell_mv_low: 0,
            cell_temp_high: 0.0,
            cell_temp_low: 0.0,
            // cells_millivolts: [0; 96],
            // cell_balance: [false; 96],
            amps: 0.0,
            kwh: 0.0,
            charge: 0.0,
            discharge: 0.0,
            bal: 0,
            valid: false,
        }
    }
}
