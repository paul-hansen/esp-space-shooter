use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::{Baseline, Text},
};
use esp_hal::i2c::master::I2c;
use esp_hal::time::{Duration, Instant};
use esp_println::println;
use ssd1306::{prelude::*, Ssd1306};

use crate::state::State;

type Display = Ssd1306<
    I2CInterface<I2c<'static, esp_hal::Blocking>>,
    DisplaySize128x64,
    ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>,
>;

pub struct AppConfig {
    pub i2c: I2c<'static, esp_hal::Blocking>,
    pub target_fps: u32,
}

pub struct App {
    display: Display,
    text_style: MonoTextStyle<'static, BinaryColor>,
    text_x: i32,
    text_y: i32,
    target_fps: u32,
    frame_duration: Duration,
}

impl App {
    /// Sets up the application with the initialized display
    pub fn setup(config: AppConfig) -> Self {
        println!("Initializing display...");

        // Create the display interface
        let interface = I2CInterface::new(config.i2c, 0x3C, 0x40);

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

        // Calculate frame duration from FPS (1000ms / fps)
        let frame_duration = Duration::from_millis((1000 / config.target_fps) as u64);

        let mut app = Self {
            display,
            text_style,
            text_x: 20,
            text_y: 30,
            target_fps: config.target_fps,
            frame_duration,
        };

        // Draw initial frame
        app.render();

        println!("App initialized!");
        println!("Target framerate: {} fps ({} ms per frame)", config.target_fps, 1000 / config.target_fps);
        println!("Press left/right buttons to move text");

        app
    }

    /// Main loop called every frame (target: 24 fps)
    pub fn main_loop(&mut self, state: &State) {
        let mut position_changed = false;

        // Update text position based on button state
        if state.button_left {
            self.text_x = self.text_x.saturating_sub(2); // Move left
            position_changed = true;
        }

        if state.button_right {
            self.text_x = (self.text_x + 2).min(128 - 66); // Move right
            position_changed = true;
        }

        // Only redraw if something changed
        if position_changed {
            self.render();
            println!("Text position: x={}", self.text_x);
        }
    }

    /// Renders the current frame to the display
    fn render(&mut self) {
        self.display.clear(BinaryColor::Off).unwrap();

        // Draw a horizontal line across the screen
        Line::new(Point::new(0, 50), Point::new(127, 50))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(&mut self.display)
            .unwrap();

        // Draw text
        Text::with_baseline(
            "hello world",
            Point::new(self.text_x, self.text_y),
            self.text_style,
            Baseline::Top,
        )
        .draw(&mut self.display)
        .unwrap();

        self.display.flush().unwrap();
    }

    /// Main run loop - runs at the configured framerate
    pub fn run<F>(&mut self, mut get_state: F) -> !
    where
        F: FnMut() -> State,
    {
        loop {
            let frame_start = Instant::now();

            // Get current input state
            let state = get_state();

            // Run frame logic
            self.main_loop(&state);

            // Wait for next frame
            while frame_start.elapsed() < self.frame_duration {}
        }
    }
}
