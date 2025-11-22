use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Triangle},
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
    sleep_frame_duration: Duration, // 4 fps when sleeping
    sleep_timeout: Duration,
    last_input_time: Instant,
    is_sleeping: bool,
    bullets: heapless::Vec<(i32, i32), 16>, // Store up to 16 bullets (x, y)
    bullet_cooldown: u32, // Frames until next bullet
    asteroids: heapless::Vec<Asteroid, 8>, // Store up to 8 asteroids
    asteroid_cooldown: u32, // Frames until next asteroid
    frame_count: u32, // For pseudo-random number generation
    score: u32, // Number of asteroids destroyed
    high_score: u32, // Highest score reached
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

        // Calculate frame duration from FPS (1000ms / fps)
        let frame_duration = Duration::from_millis((1000 / config.target_fps) as u64);
        // Sleep frame rate is 4 fps (250ms per frame) to save power while still checking for input
        let sleep_frame_duration = Duration::from_millis(250);

        // Calculate timeout duration
        let sleep_timeout = Duration::from_secs(config.sleep_timeout_secs as u64);

        // Load high score from flash storage
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
        };

        // Draw initial frame
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

        let mut needs_redraw = false;
        self.frame_count = self.frame_count.wrapping_add(1);

        // Update triangle position based on button input
        if state.button_left {
            self.triangle_x = self.triangle_x.saturating_sub(3).max(8);
            needs_redraw = true;
        }

        if state.button_right {
            self.triangle_x = (self.triangle_x + 3).min(120);
            needs_redraw = true;
        }

        // Update last input time if there was input
        if has_input {
            self.last_input_time = Instant::now();
        }

        // Handle bullet cooldown and spawning
        if self.bullet_cooldown > 0 {
            self.bullet_cooldown -= 1;
        } else {
            // Spawn a new bullet from the triangle's position
            let _ = self.bullets.push((self.triangle_x, self.triangle_y - 4));
            self.bullet_cooldown = 10; // Spawn every 10 frames
            needs_redraw = true;
        }

        // Update bullet positions (move up)
        let mut i = 0;
        while i < self.bullets.len() {
            self.bullets[i].1 -= 4; // Move up by 4 pixels

            // Remove bullets that went off screen
            if self.bullets[i].1 < -5 {
                self.bullets.swap_remove(i);
            } else {
                i += 1;
            }
            needs_redraw = true;
        }

        // Handle asteroid cooldown and spawning
        if self.asteroid_cooldown > 0 {
            self.asteroid_cooldown -= 1;
        } else {
            // Spawn a new asteroid at random x position at top of screen
            // Use frame_count for pseudo-random positioning
            let x = ((self.frame_count * 17 + 13) % 108) as i32 + 10; // Between 10 and 118
            let radius = ((self.frame_count * 7) % 3 + 3) as u32; // Radius between 3 and 5
            let _ = self.asteroids.push(Asteroid { x, y: -10, radius });
            self.asteroid_cooldown = 40; // Spawn every ~1.3 seconds at 30fps
            needs_redraw = true;
        }

        // Update asteroid positions (move down)
        let mut i = 0;
        while i < self.asteroids.len() {
            self.asteroids[i].y += 1; // Move down by 1 pixel

            // Remove asteroids that went off screen
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
                    // Collision! Remove asteroid and increment score
                    self.asteroids.swap_remove(asteroid_idx);
                    self.score += 1;
                    if self.score > self.high_score {
                        self.high_score = self.score;
                        // Save new high score to flash
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
                // Remove bullet
                self.bullets.swap_remove(bullet_idx);
            } else {
                bullet_idx += 1;
            }
        }

        // Check collisions between asteroids and triangle
        let mut i = 0;
        while i < self.asteroids.len() {
            let asteroid = &self.asteroids[i];

            // Check if asteroid is close to triangle
            let dx = asteroid.x - self.triangle_x;
            let dy = asteroid.y - self.triangle_y;
            let dist_sq = dx * dx + dy * dy;
            let collision_dist = (asteroid.radius as i32 + 4) * (asteroid.radius as i32 + 4); // radius + triangle size

            if dist_sq < collision_dist {
                // Collision with triangle! Reset score and remove asteroid
                self.score = 0;
                self.asteroids.swap_remove(i);
                needs_redraw = true;
                println!("Hit by asteroid! Score reset to 0");
            } else {
                i += 1;
            }
        }

        // Redraw if anything changed
        if needs_redraw {
            self.render();
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

        // Draw asteroids (outlined circles)
        for asteroid in &self.asteroids {
            Circle::new(Point::new(asteroid.x - asteroid.radius as i32, asteroid.y - asteroid.radius as i32), asteroid.radius * 2)
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                .draw(&mut self.display)
                .unwrap();
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

            // Get current input state
            let state = get_state();

            // Run frame logic
            self.main_loop(&state);

            // Use 4 fps when sleeping, normal fps when active
            let target_duration = if self.is_sleeping {
                self.sleep_frame_duration
            } else {
                self.frame_duration
            };

            // Wait for next frame
            while frame_start.elapsed() < target_duration {}
        }
    }
}
