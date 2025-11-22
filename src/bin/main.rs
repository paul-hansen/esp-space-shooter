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
use esp_hal::time::Rate;
use esp_println::println;

use esp_racing::app::{App, AppConfig};
use esp_racing::state::State;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    println!("ESP Racing - Initializing...");

    // Configure button pins - pull-up resistors, active low (pressed = LOW)
    let button_left = Input::new(peripherals.GPIO18, InputConfig::default().with_pull(Pull::Up));
    let button_right = Input::new(peripherals.GPIO19, InputConfig::default().with_pull(Pull::Up));

    println!("Buttons configured on GPIO18 (left) and GPIO19 (right)");

    // Configure I2C - SDA on GPIO21, SCL on GPIO22
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(400_000)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO21)
    .with_scl(peripherals.GPIO22);

    // Setup the app with configuration
    let mut app = App::setup(AppConfig {
        i2c,
        target_fps: 30,
    });

    // Run the main loop
    app.run(|| State {
        button_left: button_left.is_low(),
        button_right: button_right.is_low(),
    })
}
