use std::time::Instant;

pub const DEFAULT_FPS: u16 = 60;
const SAMPLE_SIZE: u16 = 60;
const SAMPLE_COUNT: u16 = 30;

pub struct Fps {
    fps: u16,
    frames: u16,
    samples: u16,
    fps_unlocked: bool,
    start: Option<Instant>,
}

impl Fps {
    pub const fn new() -> Fps {
        Fps {
            fps: DEFAULT_FPS,
            frames: 0,
            samples: 0,
            fps_unlocked: false,
            start: None,
        }
    }

    pub fn tick(&mut self) {
        if self.freezed() {
            return;
        }

        self.frames += 1;
        let start = self.start.unwrap_or_else(||{
            let now = Instant::now();
            self.start = Some(now);
            now
        });

        if self.frames == SAMPLE_SIZE {
            self.samples += 1;
            let now = Instant::now();
            let elapsed = now - start;
            self.fps = (self.frames as u128 * 1000 / elapsed.as_millis()) as u16;
            self.frames = 0;
            self.start = Some(now);
            
            if self.fps > DEFAULT_FPS + 5 {
                self.fps_unlocked = true;
            }

            if self.samples == SAMPLE_COUNT {
                log::debug!("FPS is {}.", if self.fps_unlocked { "unlocked" } else { "freezed" } );
            } else if self.samples < SAMPLE_COUNT {
                log::trace!("FPS: {}", self.fps);
            }
        }

    }

    pub fn get(&self) -> u16 {
        if self.freezed() {
            return DEFAULT_FPS;
        }
        self.fps.max(DEFAULT_FPS)
    }

    fn freezed(&self) -> bool {
        self.samples > SAMPLE_COUNT && !self.fps_unlocked
    }
}

pub trait FrameCount {
    fn adjust_to(self, fps: u16) -> u16; 
}
impl FrameCount for u16 {
    fn adjust_to(self, fps: u16) -> u16 {
        self * fps / DEFAULT_FPS
    }
}