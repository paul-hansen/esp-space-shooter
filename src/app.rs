use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Triangle},
    text::Text,
};
use esp_hal::i2c::master::I2c;
use esp_hal::time::{Duration, Instant};
use esp_println::println;
use ssd1306::{prelude::*, Ssd1306};

use crate::state::State;
use crate::storage;

type Display = Ssd1306<
    I2CInterface<I2c<'static, esp_hal::Blocking>>,
    DisplaySize128x64,
    ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>,
>;

struct Asteroid {
    x: i32,
    y: i32,
    radius: u32,
    seed: u32,
}

pub struct AppConfig {
    pub i2c: I2c<'static, esp_hal::Blocking>,
    pub target_fps: u32,
    /// Seconds of inactivity before entering sleep mode (display off + 4 fps, 0 = disabled)
    pub sleep_timeout_secs: u32,
}

pub struct App {
    display: Display,
    triangle_x: i32,
    triangle_y: i32,
    frame_duration: Duration,
    sleep_frame_duration: Duration,
    sleep_timeout: Duration,
    last_input_time: Instant,
    is_sleeping: bool,
    bullets: heapless::Vec<(i32, i32), 16>,
    bullet_cooldown: u32,
    asteroids: heapless::Vec<Asteroid, 8>,
    asteroid_cooldown: u32,
    frame_count: u32,
    score: u32,
    high_score: u32,
    both_buttons_held_start: Option<Instant>,
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

        let frame_duration = Duration::from_millis((1000 / config.target_fps) as u64);
        // Sleep frame rate is 4 fps (250ms per frame) to save power while still checking for input
        let sleep_frame_duration = Duration::from_millis(250);

        let sleep_timeout = Duration::from_secs(config.sleep_timeout_secs as u64);

        let saved_high_score = storage::load_high_score();
        println!("Loaded high score from flash: {}", saved_high_score);

        let mut app = Self {
            display,
            triangle_x: 64,
            triangle_y: 58, // Start near bottom of screen
            frame_duration,
            sleep_frame_duration,
            sleep_timeout,
            last_input_time: Instant::now(),
            is_sleeping: false,
            bullets: heapless::Vec::new(),
            bullet_cooldown: 0,
            asteroids: heapless::Vec::new(),
            asteroid_cooldown: 30, // First asteroid after 1 second
            frame_count: 0,
            score: 0,
            high_score: saved_high_score,
            both_buttons_held_start: None,
        };

        app.render();

        println!("App initialized!");
        println!("Target framerate: {} fps ({} ms per frame)", config.target_fps, 1000 / config.target_fps);
        if config.sleep_timeout_secs > 0 {
            println!("Sleep timeout: {} seconds (display off + 4 fps)", config.sleep_timeout_secs);
        } else {
            println!("Power saving: disabled");
        }
        println!("Use buttons to move triangle left/right. Auto-shooting bullets!");

        app
    }

    /// Main loop called every frame
    pub fn main_loop(&mut self, state: &State) {
        let has_input = state.button_left || state.button_right;
        let elapsed = self.last_input_time.elapsed();

        // Check if we should wake up from sleep
        if self.is_sleeping && has_input {
            println!("Waking from sleep");
            self.is_sleeping = false;
            self.last_input_time = Instant::now();
            self.display.set_display_on(true).unwrap();
            self.render();
            return;
        }

        // Check if we should enter sleep mode
        if !self.is_sleeping
            && self.sleep_timeout.as_millis() > 0
            && elapsed > self.sleep_timeout
        {
            println!("Entering sleep mode (display off, checking inputs at 4 fps)");
            self.is_sleeping = true;
            self.display.set_display_on(false).unwrap();
            return;
        }

        // Skip processing if sleeping
        if self.is_sleeping {
            return;
        }

        // Check if both buttons are held for high score reset
        let both_buttons = state.button_left && state.button_right;
        let mut show_reset_warning = false;

        if both_buttons {
            if self.both_buttons_held_start.is_none() {
                self.both_buttons_held_start = Some(Instant::now());
            }

            let held_duration = self.both_buttons_held_start.unwrap().elapsed();
            if held_duration >= Duration::from_secs(15) {
                self.high_score = 0;
                if let Err(e) = storage::save_high_score(0) {
                    println!("Failed to clear high score: {:?}", e);
                } else {
                    println!("High score cleared!");
                }
                self.both_buttons_held_start = None;
                self.render();
                return;
            } else if held_duration >= Duration::from_secs(10) {
                show_reset_warning = true;
            }
        } else {
            self.both_buttons_held_start = None;
        }

        let mut needs_redraw = false;
        self.frame_count = self.frame_count.wrapping_add(1);

        if state.button_left && !both_buttons {
            self.triangle_x = self.triangle_x.saturating_sub(3).max(8);
            needs_redraw = true;
        }

        if state.button_right && !both_buttons {
            self.triangle_x = (self.triangle_x + 3).min(120);
            needs_redraw = true;
        }

        if has_input {
            self.last_input_time = Instant::now();
        }

        if self.bullet_cooldown > 0 {
            self.bullet_cooldown -= 1;
        } else {
            let _ = self.bullets.push((self.triangle_x, self.triangle_y - 4));
            self.bullet_cooldown = 10; // Spawn every 10 frames
            needs_redraw = true;
        }

        let mut i = 0;
        while i < self.bullets.len() {
            self.bullets[i].1 -= 4;

            if self.bullets[i].1 < -5 {
                self.bullets.swap_remove(i);
            } else {
                i += 1;
            }
            needs_redraw = true;
        }

        if self.asteroid_cooldown > 0 {
            self.asteroid_cooldown -= 1;
        } else {
            // Use frame_count for pseudo-random positioning
            let x = ((self.frame_count * 17 + 13) % 108) as i32 + 10; // Between 10 and 118
            let radius = ((self.frame_count * 7) % 3 + 3) as u32; // Radius between 3 and 5
            let seed = self.frame_count.wrapping_mul(1103515245).wrapping_add(12345);
            let _ = self.asteroids.push(Asteroid { x, y: -10, radius, seed });
            self.asteroid_cooldown = 40; // Spawn every ~1.3 seconds at 30fps
            needs_redraw = true;
        }

        let mut i = 0;
        while i < self.asteroids.len() {
            self.asteroids[i].y += 1;

            if self.asteroids[i].y > 70 {
                self.asteroids.swap_remove(i);
            } else {
                i += 1;
            }
            needs_redraw = true;
        }

        // Check collisions between bullets and asteroids
        let mut bullet_idx = 0;
        while bullet_idx < self.bullets.len() {
            let (bx, by) = self.bullets[bullet_idx];
            let mut hit = false;
            let mut asteroid_idx = 0;

            while asteroid_idx < self.asteroids.len() {
                let asteroid = &self.asteroids[asteroid_idx];

                // Simple distance-based collision detection
                let dx = bx - asteroid.x;
                let dy = by - asteroid.y;
                let dist_sq = dx * dx + dy * dy;
                let collision_dist = (asteroid.radius as i32 + 2) * (asteroid.radius as i32 + 2); // radius + bullet size

                if dist_sq < collision_dist {
                    self.asteroids.swap_remove(asteroid_idx);
                    self.score += 1;
                    if self.score > self.high_score {
                        self.high_score = self.score;
                        if let Err(e) = storage::save_high_score(self.high_score) {
                            println!("Failed to save high score: {:?}", e);
                        } else {
                            println!("New high score saved: {}", self.high_score);
                        }
                    }
                    hit = true;
                    needs_redraw = true;
                    break;
                } else {
                    asteroid_idx += 1;
                }
            }

            if hit {
                self.bullets.swap_remove(bullet_idx);
            } else {
                bullet_idx += 1;
            }
        }

        // Check collisions between asteroids and triangle
        let mut i = 0;
        while i < self.asteroids.len() {
            let asteroid = &self.asteroids[i];

            let dx = asteroid.x - self.triangle_x;
            let dy = asteroid.y - self.triangle_y;
            let dist_sq = dx * dx + dy * dy;
            let collision_dist = (asteroid.radius as i32 + 4) * (asteroid.radius as i32 + 4); // radius + triangle size

            if dist_sq < collision_dist {
                self.score = 0;
                self.asteroids.swap_remove(i);
                needs_redraw = true;
                println!("Hit by asteroid! Score reset to 0");
            } else {
                i += 1;
            }
        }

        if needs_redraw || show_reset_warning {
            self.render();
        }
    }

    /// Draw an asteroid with an irregular shape using individual pixels
    fn draw_asteroid(&mut self, x: i32, y: i32, radius: u32, seed: u32) {
        let r = radius as i32;

        // Draw an irregular asteroid using circle points with pseudo-random variations
        // Using Bresenham-like approach with 8 octants for efficiency
        let mut oct_x = r;
        let mut oct_y = 0;
        let mut decision = 1 - r;

        while oct_x >= oct_y {
            // For each of the 8 octants, draw with pseudo-random variation
            let points = [
                (oct_x, oct_y), (oct_y, oct_x),
                (-oct_x, oct_y), (-oct_y, oct_x),
                (-oct_x, -oct_y), (-oct_y, -oct_x),
                (oct_x, -oct_y), (oct_y, -oct_x),
            ];

            for (i, &(dx, dy)) in points.iter().enumerate() {
                // Create pseudo-random variation for each point
                let point_seed = seed
                    .wrapping_add((dx + dy * 256 + i as i32 * 17) as u32)
                    .wrapping_mul(1103515245)
                    .wrapping_add(12345);
                let variation = ((point_seed >> 16) % 3) as i32 - 1;

                let px = x + dx + variation;
                let py = y + dy + variation;

                // Draw pixel cluster for rocky look
                for pdx in -1..=1 {
                    for pdy in -1..=1 {
                        let final_x = px + pdx;
                        let final_y = py + pdy;

                        if final_x >= 0 && final_x < 128 && final_y >= 0 && final_y < 64 {
                            if pdx * pdx + pdy * pdy <= 1 {
                                self.display.set_pixel(final_x as u32, final_y as u32, true);
                            }
                        }
                    }
                }
            }

            oct_y += 1;
            if decision < 0 {
                decision += 2 * oct_y + 1;
            } else {
                oct_x -= 1;
                decision += 2 * (oct_y - oct_x) + 1;
            }
        }

        // Fill interior with scattered pixels for texture
        for i in 0..(r * 3) {
            let pixel_seed = seed.wrapping_add((i * 13) as u32).wrapping_mul(1103515245).wrapping_add(12345);
            let offset_x = ((pixel_seed >> 8) % (r as u32 * 2)) as i32 - r;
            let offset_y = ((pixel_seed >> 16) % (r as u32 * 2)) as i32 - r;

            let px = x + offset_x;
            let py = y + offset_y;

            let dist_sq = offset_x * offset_x + offset_y * offset_y;
            if dist_sq < (r - 1) * (r - 1) && px >= 0 && px < 128 && py >= 0 && py < 64 {
                self.display.set_pixel(px as u32, py as u32, true);
            }
        }
    }

    /// Renders the current frame to the display
    fn render(&mut self) {
        self.display.clear(BinaryColor::Off).unwrap();

        // Draw score in top left
        use core::fmt::Write;
        let mut score_text: heapless::String<16> = heapless::String::new();
        write!(&mut score_text, "{}", self.score).unwrap();

        let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        Text::new(&score_text, Point::new(2, 8), text_style)
            .draw(&mut self.display)
            .unwrap();

        // Draw high score in top right
        let mut hs_text: heapless::String<16> = heapless::String::new();
        write!(&mut hs_text, "HS:{}", self.high_score).unwrap();

        // Position text on the right side (128px wide screen, font is 6px wide)
        let text_width = hs_text.len() as i32 * 6;
        Text::new(&hs_text, Point::new(128 - text_width - 2, 8), text_style)
            .draw(&mut self.display)
            .unwrap();

        // Draw warning if both buttons held for 10+ seconds
        if let Some(start_time) = self.both_buttons_held_start {
            let held_duration = start_time.elapsed();
            if held_duration >= Duration::from_secs(10) {
                let remaining = 15 - held_duration.as_secs();
                let mut warning_text: heapless::String<32> = heapless::String::new();
                write!(&mut warning_text, "Score Reset in {}", remaining).unwrap();

                let warning_width = warning_text.len() as i32 * 6;
                let x_pos = (128 - warning_width) / 2;
                Text::new(&warning_text, Point::new(x_pos, 32), text_style)
                    .draw(&mut self.display)
                    .unwrap();
            }
        }

        // Draw asteroids with irregular shapes
        for i in 0..self.asteroids.len() {
            let x = self.asteroids[i].x;
            let y = self.asteroids[i].y;
            let radius = self.asteroids[i].radius;
            let seed = self.asteroids[i].seed;
            self.draw_asteroid(x, y, radius, seed);
        }

        // Draw bullets (5px vertical lines)
        for &(x, y) in &self.bullets {
            Line::new(Point::new(x, y), Point::new(x, y + 5))
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                .draw(&mut self.display)
                .unwrap();
        }

        // Draw a filled triangle
        let size = 4;
        Triangle::new(
            Point::new(self.triangle_x, self.triangle_y - size),     // Top point
            Point::new(self.triangle_x - size, self.triangle_y + size), // Bottom left
            Point::new(self.triangle_x + size, self.triangle_y + size), // Bottom right
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(&mut self.display)
        .unwrap();

        self.display.flush().unwrap();
    }

    /// Main run loop - runs at the configured framerate
    /// Uses 4 fps when sleeping to save power
    pub fn run<F>(&mut self, mut get_state: F) -> !
    where
        F: FnMut() -> State,
    {
        loop {
            let frame_start = Instant::now();

            let state = get_state();
            self.main_loop(&state);

            let target_duration = if self.is_sleeping {
                self.sleep_frame_duration
            } else {
                self.frame_duration
            };

            while frame_start.elapsed() < target_duration {}
        }
    }
}
