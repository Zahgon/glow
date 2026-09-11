//! The animated spinner.

use std::time::Duration;

use crate::deps::lipgloss::Style;

/// A set of frames and the rate they advance at.
#[derive(Debug, Clone)]
pub struct Spinner {
    /// The frames, in order.
    pub frames: &'static [&'static str],
    /// How long each frame lasts.
    pub fps: Duration,
}

/// The `Line` spinner glow uses.
pub const LINE: Spinner = Spinner {
    frames: &["|", "/", "-", "\\"],
    fps: Duration::from_millis(100),
};

/// The spinner model.
#[derive(Debug, Clone)]
pub struct Model {
    /// Frames and rate.
    pub spinner: Spinner,
    /// Styling for the frame.
    pub style: Style,
    frame: usize,
}

impl Default for Model {
    fn default() -> Model {
        Model {
            spinner: LINE,
            style: Style::new(),
            frame: 0,
        }
    }
}

impl Model {
    /// A spinner with the default frames.
    pub fn new() -> Model {
        Model::default()
    }

    /// How long until the next frame.
    pub fn fps(&self) -> Duration {
        self.spinner.fps
    }

    /// Advances one frame.
    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % self.spinner.frames.len();
    }

    /// Renders the current frame.
    pub fn view(&self) -> String {
        match self.spinner.frames.get(self.frame) {
            Some(f) => self.style.render(f),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cycle() {
        let mut s = Model::new();
        assert_eq!(s.view(), "|");
        s.tick();
        assert_eq!(s.view(), "/");
        s.tick();
        s.tick();
        assert_eq!(s.view(), "\\");
        s.tick();
        assert_eq!(s.view(), "|", "the frames wrap around");
    }

    #[test]
    fn the_line_spinner_runs_at_ten_frames_a_second() {
        assert_eq!(Model::new().fps(), Duration::from_millis(100));
    }
}
