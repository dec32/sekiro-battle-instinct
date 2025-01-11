use std::fmt::Debug;

use arrayvec::ArrayVec;
use log::trace;
use Input::*;

// buffer behavior
const INPUTS_CAP: usize = 3;
const MAX_INTERVAL: u8 = 15;
const MAX_ATTACK_DELAY: u8 = 25;
// joystick ergonomics
const MAX_DISTANCE: u16 = i16::MAX as u16;
const ORTHO_THRESHOLD: u16 = MAX_DISTANCE / 100 * 85;
const DIAGO_THRESHOLD: u16 = MAX_DISTANCE / 100 * 40;
const ROTATE_THRESHOLD: u16 = MAX_DISTANCE / 100 * 90;
const BOUNCE_THRESHOLD: u16 = MAX_DISTANCE / 100 * 40;


/// I love type safety and readability.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum Input {
    // orthogonal, possible on both keyboards and gamepads
    Up = 0, Rt = 2, Dn = 4, Lt = 6, 
    // diagonal, only possible on gamepads
    Ur = 1, Dr = 3, Dl = 5, Ul = 7,
}

impl From<usize> for Input {
    fn from(value: usize) -> Self {
        match value {
            0 => Up, 2 => Rt, 4 => Dn, 6 => Lt,
            1 => Ur, 3 => Dr, 5 => Dl, 7 => Ul,
            _ => panic!("If you see this message, the programmer of this MOD is an idiot.")
        }
    }
}

impl Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up => write!(f, "↑"),
            Self::Rt => write!(f, "→"),
            Self::Dn => write!(f, "↓"),
            Self::Lt => write!(f, "←"),
            Self::Ur => write!(f, "↗"),
            Self::Dr => write!(f, "↘"),
            Self::Dl => write!(f, "↙"),
            Self::Ul => write!(f, "↖"),
        }
    }
}

impl Input {
    pub fn opposite(self) -> Input {
        Input::from((self as usize + 4) % 8)
    }

    fn is_diagonal(self) -> bool {
        self as usize % 2 == 1
    }

    fn digit(self) -> usize {
        self as usize + 1
    }
}


/// A stack-allocated container for input sequences
pub type Inputs = ArrayVec<Input, INPUTS_CAP>;
pub trait InputsExt {
    fn meant_for_art(&self) -> bool;
}
impl InputsExt for Inputs {
    fn meant_for_art(&self) -> bool {
        self.len() >= 2
    }
}

/// A input buffer that remembers the most recent 3 motion inputs
/// The buffer expires after several frames unless new inputs are pushed into it and reset its age
pub struct InputBuffer {
    inputs: Inputs,
    keys_down: [bool; 4],
    neutral: bool,
    allow_diagonal: bool,
    frames: u8,
}

impl InputBuffer {
    pub const fn new() -> InputBuffer {
        InputBuffer {
            inputs: Inputs::new_const(),
            keys_down: [false; 4],
            neutral: true,
            allow_diagonal: false,
            frames: 0,
        }
    }

    // TODO it should tell its caller if the inputs expired or not
    pub fn update_keys(&mut self, up: bool, right: bool, down: bool, left: bool) -> Inputs {
        let mut updated = false;
        for (i, down) in [up, right, down, left].iter().cloned().enumerate() {
            if !self.keys_down[i] && down {
                // newly pressed key
                self.push(Input::from(i * 2));
                updated = true;
            }
            self.keys_down[i] = down;
        }
        self.incr_frames(updated);
        self.inputs.clone()
    }

    pub fn update_joystick(&mut self, x: i16, y: i16) -> Inputs {
        let mut updated = false;
        let x_abs = x.unsigned_abs();
        let y_abs = y.unsigned_abs();
        let input = if x_abs > DIAGO_THRESHOLD && y_abs > DIAGO_THRESHOLD && self.allow_diagonal {
            let input = match(x, y) {
                (0.., 0..) => Ur,
                (0.., _  ) => Dr,
                (_ ,  0..) => Ul,
                (_ ,  _  ) => Dl,
            };
            Some(input)
        } else {
            let input = if y_abs >= x_abs {
                if y > 0 { Up } else { Dn }
            } else {
                if x > 0 { Rt } else { Lt } 
            };
            let chebyshev_distance = u16::max(x_abs, y_abs);
            let threshold = if let Some(last) = self.inputs.last().cloned() {
                if input == last {
                    ORTHO_THRESHOLD
                } else if input == last.opposite() {
                    // makes bouncing inputs (↑↓, ↓↑, ←→, →←) easier by using a smaller threshold
                    BOUNCE_THRESHOLD
                } else {
                    // makes rotating inputs (↑→, →↓, ↓←, ←↑) HARDER by using a bigger threshold
                    ROTATE_THRESHOLD
                }
            } else {
                ORTHO_THRESHOLD
            };
            if chebyshev_distance >= threshold {
                Some(input)
            } else {
                None
            }
        };
        
        if let Some(input) = input {
            if self.neutral || !self.inputs.ends_with(&[input]) {
                self.push(input);
                updated = true;
            }
            self.neutral = false;
        } else {
            self.neutral = true;
        }
        self.incr_frames(updated);
        self.inputs.clone()
    }

    fn push(&mut self, input: Input) {
        if self.frames > MAX_INTERVAL {
            self.inputs.clear();
        }
        // diagonal inputs never take part in sequences
        if input.is_diagonal() && !self.inputs.is_empty() {
            trace!("Denied {input:?}");
            return;
        }
        // thus when other inputs happens, the leading diagonal input gets kicked out
        if self.inputs.first().into_iter().any(|first|first.is_diagonal()) || self.inputs.is_full(){
            self.inputs.remove(0);
        }
        self.inputs.push(input);
        trace!("{:?}({}) ", self.inputs, self.frames);
    }

    fn incr_frames(&mut self, updated: bool) {
        if updated {
            self.frames = 0;
        } else {
            self.frames = self.frames.saturating_add(1);
        }
    }

    pub fn aborted(&self) -> bool {
        self.keys_down == [false, false, false, false] && self.neutral && self.frames >= MAX_ATTACK_DELAY
    }

    pub fn clear(&mut self) {
        self.inputs.clear();
        self.frames = 0;
    }
}


/// An array-based trie that uses input sequence as keys
pub struct InputsTrie<T> {
    array: [Option<T>; usize::pow(9, INPUTS_CAP as u32)]
}

impl <T:Copy>InputsTrie<T> {
    pub const fn new() -> InputsTrie<T> {
        InputsTrie {
            array: [None; usize::pow(9, INPUTS_CAP as u32)]
        }
    }

    pub fn insert(&mut self, inputs: Inputs, ele: T) {
        self.array[Self::idx(&inputs)] = Some(ele);
    }

    pub fn get(&self, inputs: &[Input]) -> Option<T> {
        self.array[Self::idx(inputs)]
    }

    fn idx(inputs: &[Input]) -> usize {
        // cast the input sequence into a base-9 number
        const BASE: usize = 9;
        let mut idx = 0;
        for (i, input) in inputs.iter().cloned().take(INPUTS_CAP).enumerate() {
            idx += input.digit() * BASE.pow(i as u32);
        }
        idx
    }
}

