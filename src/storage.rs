use embedded_storage::{ReadStorage, Storage};
use esp_storage::{FlashStorage, FlashStorageError};

/// Flash address offset where we store the high score
/// This is in the NVS-like area, far from program code
const HIGH_SCORE_ADDR: u32 = 0x9000;

/// Magic number to verify the high score data is valid
const MAGIC: u32 = 0xDEADBEEF;

/// Structure stored in flash
#[repr(C)]
struct HighScoreData {
    magic: u32,
    score: u32,
}

/// Load the high score from flash storage
/// Returns 0 if no valid high score is found
pub fn load_high_score() -> u32 {
    let mut flash = FlashStorage::new();
    let mut buffer = [0u8; 8]; // 4 bytes for magic + 4 bytes for score

    match flash.read(HIGH_SCORE_ADDR, &mut buffer) {
        Ok(_) => {
            let magic = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
            let score = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);

            if magic == MAGIC {
                score
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

/// Save the high score to flash storage
pub fn save_high_score(score: u32) -> Result<(), FlashStorageError> {
    let mut flash = FlashStorage::new();

    let data = HighScoreData {
        magic: MAGIC,
        score,
    };

    let mut buffer = [0u8; 8];
    buffer[0..4].copy_from_slice(&data.magic.to_le_bytes());
    buffer[4..8].copy_from_slice(&data.score.to_le_bytes());

    // Write the data (esp-storage handles erase internally)
    flash.write(HIGH_SCORE_ADDR, &buffer)?;

    Ok(())
}
