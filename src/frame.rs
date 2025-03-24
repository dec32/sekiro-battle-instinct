use std::{cell::UnsafeCell, mem, time::Instant};

pub const DEFAULT_FRAMERATE: u16 = 60;

/// Frame count under the standard FPS as a time unit, namely, 1/60s.
/// It can be adjusted to the current framerate
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Frames(u16);
impl Frames {
    #[inline(always)]
    pub const fn standard(value: u16) -> Frames {
        Frames(value)
    }
    #[inline(always)]
    pub fn as_actual(self) -> u16 {
        self.0 * FRAMERATE.cur() / DEFAULT_FRAMERATE
    }
    #[allow(unused)]
    #[inline(always)]
    pub fn as_standard(self) -> u16 {
        self.0
    }
}

/// A tracker that tracks the in-game framerate.
#[repr(transparent)]
pub struct Framerate(UnsafeCell<FramerateInner>);
unsafe impl Sync for Framerate {}

impl Framerate {
    const fn new() -> Framerate {
        Framerate(UnsafeCell::new(FramerateInner::new()))
    }

    #[inline(always)]
    pub fn cur(&self) -> u16 {
        unsafe { self.as_mut().cur() }
    }

    /// Safety: this function should only be hooked into the main tick of the game, but never
    /// called directly from arbitrary threads
    #[inline(always)]
    pub unsafe fn tick(&self) {
        unsafe { self.as_mut().tick() }
    }

    #[inline(always)]
    unsafe fn as_mut(&self) -> &mut FramerateInner {
        unsafe { mem::transmute(self.0.get()) }
    }
}

struct FramerateInner {
    // stores the recent framerate
    cur: u16,
    // passing frames since `since`
    frames: u16,
    since: Option<Instant>,
    // stores how many framerate value is sampled
    samples: u16,
    unlocked: bool,
}

impl FramerateInner {
    const SAMPLE_SIZE: u16 = 60;
    const SAMPLE_COUNT: u16 = 30;

    const fn new() -> FramerateInner {
        FramerateInner { cur: DEFAULT_FRAMERATE, frames: 0, samples: 0, unlocked: false, since: None }
    }

    fn tick(&mut self) {
        if self.is_freezed() {
            return;
        }

        let since = *self.since.get_or_insert_with(Instant::now);
        self.frames += 1;
        if self.frames < Self::SAMPLE_SIZE {
            return;
        }

        let now = Instant::now();
        let elapsed = now - since;
        self.cur = (self.frames as u128 * 1000 / elapsed.as_millis()) as u16;
        self.frames = 0;
        self.since = Some(now);
        self.samples = self.samples.saturating_add(1);
        
        if self.cur > DEFAULT_FRAMERATE + 5 {
            self.unlocked = true;
        }

        if self.samples == Self::SAMPLE_COUNT {
            if self.unlocked {
                log::debug!("Framerate is unlocked.");
            } else {
                self.cur = DEFAULT_FRAMERATE;
                log::debug!("Framerate is freezed.");
            }
        } else if self.samples < Self::SAMPLE_COUNT {
            log::trace!("Framerate: {}", self.cur);
            return;
        } 
    }

    fn cur(&self) -> u16 {
        self.cur.max(DEFAULT_FRAMERATE)
    }

    fn is_freezed(&self) -> bool {
        self.samples >= Self::SAMPLE_COUNT && !self.unlocked
    }
}

/// The global framerate tracker
pub static FRAMERATE: Framerate = Framerate::new();
