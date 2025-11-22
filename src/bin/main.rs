#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::main;
use esp_hal::time::{Duration, Instant, Rate};
use esp_println::println;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 1.0.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Configure button pins - pull-up resistors, active low (pressed = LOW)
    let button_left = Input::new(peripherals.GPIO18, InputConfig::default().with_pull(Pull::Up));
    let button_right = Input::new(peripherals.GPIO19, InputConfig::default().with_pull(Pull::Up));

    println!("Buttons configured on GPIO18 (left) and GPIO19 (right)");

    println!("Initializing I2C...");

    // Configure I2C - SDA on GPIO21, SCL on GPIO22
    // 400kHz = 400000Hz
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(400_000)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO21)
    .with_scl(peripherals.GPIO22);

    println!("Initializing display...");

    // Create the display interface
    let interface = I2CDisplayInterface::new(i2c);

    // Create the display driver
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().unwrap();
    display.clear(BinaryColor::Off).unwrap();

    // Create text style
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    // Initial text position
    let mut text_x = 20_i32;
    let text_y = 30;

    // Draw initial text
    display.clear(BinaryColor::Off).unwrap();
    Text::with_baseline("hello world", Point::new(text_x, text_y), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();
    display.flush().unwrap();

    println!("Display initialized! Showing 'hello world'");
    println!("Press left/right buttons to move text");

    loop {
        let mut position_changed = false;

        // Read button states (LOW = pressed due to pull-up resistors)
        if button_left.is_low() {
            text_x = text_x.saturating_sub(2); // Move left, don't go below 0
            position_changed = true;
            println!("Left button pressed, x={}", text_x);
        }

        if button_right.is_low() {
            text_x = (text_x + 2).min(128 - 66); // Move right, don't exceed screen width
            position_changed = true;
            println!("Right button pressed, x={}", text_x);
        }

        // Redraw display if position changed
        if position_changed {
            display.clear(BinaryColor::Off).unwrap();
            Text::with_baseline("hello world", Point::new(text_x, text_y), text_style, Baseline::Top)
                .draw(&mut display)
                .unwrap();
            display.flush().unwrap();
        }

        // Small delay for button debouncing and to prevent too-fast updates
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(50) {}
    }
}
