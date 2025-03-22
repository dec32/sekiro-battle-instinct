use std::fmt::Debug;
use Input::*;

use crate::frame::{FrameCount, DEFAULT_FPS};

// buffer behavior
const INPUTS_CAP: u8 = 3;
const MAX_INTERVAL: u16 = 10;
const MAX_ATTACK_DELAY: u16 = 10;
// joystick ergonomics
const MAX_DISTANCE: u16 = i16::MAX as u16;
const COMMON_THRESHOLD: u16 = MAX_DISTANCE / 100 * 85;
const ROTATE_THRESHOLD: u16 = MAX_DISTANCE / 100 * 90;
const BOUNCE_THRESHOLD: u16 = MAX_DISTANCE / 100 * 40;

/// I love type safety and readability.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum Input {
    Up = 0, Right = 1, Down = 2, Left = 3,
}

impl Input {
    #[inline(always)]
    fn from_repr(repr: u8) -> Input {
        match repr {
            0 => Up, 1 => Right, 2 => Down, 3 => Left,
            _ => panic!("Illegal representation {repr}.")
        }
    }

    #[inline(always)]
    fn from_one_based(value: u8) -> Input {
        Input::from_repr(value - 1)
    }

    #[inline(always)]
    fn as_one_based(self) -> u8 {
        self as u8 + 1
    }
}

impl Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up =>    write!(f, "↑"),
            Self::Right => write!(f, "→"),
            Self::Down =>  write!(f, "↓"),
            Self::Left =>  write!(f, "←"),
        }
    }
}

impl Input {
    pub fn opposite(self) -> Input {
        Input::from_repr((self as u8 + 2) % 4)
    }

    pub fn rotate(self) -> Input {
        Input::from_repr((self as u8 + 1) % 4)
    }
}


/// An input buffer that remembers the most recent 3 motion inputs
/// The buffer expires after several frames unless new inputs are pushed into it and reset its age
pub struct InputBuffer {
    inputs: Inputs,
    frames: u16,
    neutral: bool,
    keys_down: [bool; 4],
    fps: u16,
}

impl InputBuffer {
    pub const fn new() -> InputBuffer {
        InputBuffer {
            inputs: Inputs::new(),
            frames: 0,
            neutral: true,
            keys_down: [false; 4],
            fps: DEFAULT_FPS,
        }
    }

    pub fn update_fps(&mut self, fps: u16) {
        self.fps = fps;
    }

    pub fn update_keys(&mut self, up: bool, right: bool, down: bool, left: bool) -> Inputs {
        let mut updated = false;
        for (i, (down, input)) in [(up, Up), (right, Right), (down, Down), (left, Left)].into_iter().enumerate() {
            if !self.keys_down[i] && down {
                // newly pressed key
                self.push(input);
                updated = true;
            }
            self.keys_down[i] = down;
        }
        self.incr_frames(updated);
        self.inputs
    }

    pub fn update_joystick(&mut self, x: i16, y: i16) -> Inputs {
        let mut updated = false;
        let x_abs = x.unsigned_abs();
        let y_abs = y.unsigned_abs();

        let input = if y_abs >= x_abs {
            if y > 0 { Up } else { Down }
        } else {
            if x > 0 { Right } else { Left } 
        };

        // using chebyshev distance means we have a square-shaped neutral zone
        let distance = u16::max(x_abs, y_abs);
        let threshold = if let Some(last) = self.inputs.last() {
            if input == last {
                COMMON_THRESHOLD
            } else if input == last.opposite() {
                // makes bouncing inputs (↑↓, ↓↑, ←→, →←) easier by using a smaller threshold
                BOUNCE_THRESHOLD
            } else {
                // makes rotating inputs (↑→, →↓, ↓←, ←↑) HARDER by using a bigger threshold
                ROTATE_THRESHOLD
            }
        } else {
            COMMON_THRESHOLD
        };

        if distance < threshold {
            self.neutral = true;
        } else {
            if self.neutral || self.inputs.last().into_iter().any(|last|input != last) {
                self.push(input);
                updated = true;
            }
            self.neutral = false;
        }

        self.incr_frames(updated);
        self.inputs.clone()
    }

    fn push(&mut self, input: Input) {
        let max_interval = MAX_INTERVAL.adjust_to(self.fps);
        if self.inputs.len() >= INPUTS_CAP || self.frames > max_interval {
            self.inputs.clear();
        }
        self.inputs.push(input);
    }

    fn incr_frames(&mut self, updated: bool) {
        if updated {
            self.frames = 0;
        } else {
            self.frames = self.frames.saturating_add(1);
        }
    }

    pub fn expired(&self) -> bool {
        if self.inputs.len() == 1 {
            self.neutral && self.keys_down == [false, false, false, false]
        } else {
            let max_attack_delay = MAX_ATTACK_DELAY.adjust_to(self.fps);
            self.frames >= max_attack_delay
        }
    }

    pub fn clear(&mut self) {
        self.inputs.clear();
        self.frames = 0;
    }
}


//----------------------------------------------------------------------------
//
//  An array-based trie that uses input sequence as keys
//
//----------------------------------------------------------------------------
pub struct InputsTrie<T> {
    array: [Option<T>; Inputs::MAX_HASHCODE]
}

impl<T:Copy> InputsTrie<T> {
    pub const fn new() -> InputsTrie<T> {
        InputsTrie {
            array: [None; Inputs::MAX_HASHCODE]
        }
    }

    pub fn get(&self, inputs: impl Into<Inputs>) -> Option<T> {
        self.array[inputs.into().hashcode()]
    }

    pub fn insert(&mut self, inputs: impl Into<Inputs>, value: T) {
        self.array[inputs.into().hashcode()] = Some(value);
    }

    pub fn try_insert(&mut self, inputs: impl Into<Inputs>, value: T) {
        self.array[inputs.into().hashcode()].get_or_insert(value);
    }
}

impl<T: Default + Copy> InputsTrie<T> {
    pub fn get_or_default(&self, inputs: impl Into<Inputs>) -> T {
        self.array[inputs.into().hashcode()].unwrap_or_default()
    }
}

//----------------------------------------------------------------------------
//
//  A pseudo-vec that can store a sequence of inputs in the form of the perfect
//  hash of itself.
//
//----------------------------------------------------------------------------

// todo: apparently it dosn't need to derive Hash
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Inputs {
    hashcode: u8,
    len: u8,
}

impl Inputs {
    const BASE: u8 = 5;
    const MAX_HASHCODE: usize = u8::MAX as usize;

    pub const fn new() -> Inputs {
        Inputs {
            hashcode: 0,
            len: 0
        }
    }

    pub fn push(&mut self, input: Input) {
        self.hashcode *= Self::BASE;
        self.hashcode += input.as_one_based();
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<Input> {
        let remainder = self.hashcode % Self::BASE;
        if remainder == 0 {
            None
        } else {
            self.hashcode /= Self::BASE;
            self.len -= 1;
            Some(Input::from_one_based(remainder))
        }
    }

    pub fn rev(mut self) -> Inputs {
       let mut rev = Inputs::new();
       while let Some(input) = self.pop() {
            rev.push(input);
       }
       rev
    }
 
    pub fn last(self) -> Option<Input> {
        let last_digit = self.hashcode % Self::BASE;
        if last_digit == 0 {
            None
        } else {
            Some(Input::from_one_based(last_digit))
        }
    }

    pub fn clear(&mut self) {
        self.hashcode = 0;
        self.len = 0;
    }

    pub fn len(self) -> u8 {
        self.len
    }

    pub fn hashcode(self) -> usize {
        self.hashcode as usize
    }

    pub fn meant_for_art(self) -> bool {
        self.len >= 2
    }
}


impl FromIterator<Input> for Inputs {
    fn from_iter<T: IntoIterator<Item = Input>>(iter: T) -> Self {
        let mut inputs = Inputs::new();
        let mut iter = iter.into_iter();
        while let Some(input) = iter.next() {
            inputs.push(input);
        }
        inputs
    }
}

impl<const N:usize> From<[Input;N]> for Inputs {
    fn from(array: [Input;N]) -> Self {
        Inputs::from_iter(array.into_iter())
    }
}

#[test]
fn test_inputs() {
    fn assert_hash(inputs: impl Into<Inputs>, hashcode: &str) {
        let hashcode = u8::from_str_radix(hashcode, 5).unwrap();
        assert_eq!(inputs.into().hashcode, hashcode)
    }

    assert_hash([Up], "1");
    assert_hash([Right], "2");
    assert_hash([Down], "3");
    assert_hash([Left], "4");
    assert_hash([Up, Up], "11");
    assert_hash([Up, Right], "12");
    assert_hash([Up, Right, Down], "123");

    assert_eq!(Inputs::from([]).len(), 0);
    assert_eq!(Inputs::from([Up]).len(), 1);
    assert_eq!(Inputs::from([Up, Right]).len(), 2);
    assert_eq!(Inputs::from([Up, Right, Down]).len(), 3);
}