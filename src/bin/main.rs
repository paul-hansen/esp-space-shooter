#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::main;
use esp_println::println;

use esp_asteroids::app::{App, AppConfig};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    println!("ESP Asteroids - Initializing...");

    let mut app = App::setup(
        AppConfig {
            target_fps: 30,
            sleep_timeout_secs: 10, // Sleep after 10 seconds (display off + 4 fps, 0 = disabled)
        },
    );

    app.run()
}
