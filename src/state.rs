/// Holds the current state of inputs for each frame
#[derive(Default, Debug, Clone, Copy)]
pub struct State {
    pub button_left: bool,
    pub button_right: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            button_left: false,
            button_right: false,
        }
    }
}
