use arrayvec::ArrayVec;
use log::trace;
use Input::*;

const INPUTS_CAP: usize = 3;
const MAX_INTERVAL: u8 = 20;
const MAX_ATTACK_DELAY: u8 = 10;
const JOYSTICK_THRESHOLD: u16 = (i16::MAX / 100 * 80) as u16;
const JOYSTICK_BOUNCE_THRESHOLD: u16 = (i16::MAX / 100 * 50) as u16;

/// I love type safety and readability.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Input {
    Up    = 0, 
    Right = 1,
    Down  = 2, 
    Left  = 3, 
}

impl Input {
    fn opposite(self) -> Input {
        Input::from((self as usize + 2) % 4)
    }
    
    fn quinary_digit(self) -> usize {
        self as usize + 1
    }
}

impl From<usize> for Input {
    fn from(value: usize) -> Self {
        match value {
            0 => Up,
            1 => Right,
            2 => Down,
            3 => Left,
            _ => panic!("If you see this message, the programmer of this MOD is an idiot.")
        }
    }
}


/// A stack-allocated container for input sequences
pub type Inputs = ArrayVec<Input, INPUTS_CAP>;
pub trait InputsExt {
    fn meant_for_art(&self) -> bool;
}
impl InputsExt for Inputs {
    fn meant_for_art(&self) -> bool {
        self.len() >= 3 || matches!(self.as_slice(), [Up, Up] | [Down, Down] | [Left, Left] | [Right, Right] | [Up, Down] | [Down, Up] | [Left, Right] | [Right, Left])
    }
}



/// A input buffer that remembers the most recent 3 directional inputs
/// The buffer expires after 10 frames unless new inputs are pushed into the buffer and refresh its age
pub struct InputBuffer {
    inputs: Inputs,
    keys_down: [bool; 4],
    age: u8,
}

impl InputBuffer {
    pub const fn new() -> InputBuffer {
        InputBuffer {
            inputs: Inputs::new_const(),
            keys_down: [false; 4],
            age: 0
        }
    }

    // TODO it should tell its caller if the inputs are expired or not
    pub fn update(&mut self, up: bool, right: bool, down: bool, left: bool) -> Inputs {
        let mut updated = false;
        for (i, key) in [up, right, down, left].iter().cloned().enumerate() {
            if !self.keys_down[i] && key {
                // newly pressed direction
                if self.inputs.len() >= self.inputs.capacity() || self.age > MAX_INTERVAL {
                    self.inputs.clear();
                }
                self.inputs.push(Input::from(i));
                updated = true;
            }
            self.keys_down[i] = key;
        }

        if updated {
            self.age = 0;
            trace!("Buffer: {:?}", self.inputs);
        } else {
            self.age = self.age.saturating_add(1);
        }
        self.inputs.clone()
    }

    pub fn update_pos(&mut self, x: i16, y: i16) -> Inputs {
        let x_abs = x.unsigned_abs();
        let y_abs: u16 = y.unsigned_abs();

        let input = if y_abs >= x_abs {
            if y > 0 { Up } else { Down }
        } else {
            if x > 0 { Right } else { Left } 
        };

        // using chebyshev distance means we have a square-shaped dead zone
        let chebyshev_distance = u16::max(x_abs, y_abs);
        let threshold = match self.inputs.last().cloned() {
            Some(last) => {
                if input == last.opposite() {
                    // this makes inputs like [Up, Down] or [Left, Right] a bit easier
                    JOYSTICK_BOUNCE_THRESHOLD
                } else {
                    JOYSTICK_THRESHOLD
                }
            }
            None => JOYSTICK_THRESHOLD
        };

        if chebyshev_distance < threshold as u16 {
            self.update(false, false, false, false)
        } else {
            // if matches!(chebyshev_distance, JOYSTICK_BOUNCE_THRESHOLD..=JOYSTICK_THRESHOLD) {
            //     trace!("Lazy bouncing happend")
            // }
            match input {
                Up => self.update(true, false, false, false),
                Right => self.update(false, true, false, false),
                Down => self.update(false, false, true, false),
                Left => self.update(false, false, false, true),
            }
        }
    }

    pub fn aborted(&self) -> bool {
        self.keys_down == [false, false, false, false] && self.age >= MAX_ATTACK_DELAY
    }

    pub fn clear(&mut self) {
        self.inputs.clear();
        self.age = 0;
    }
}


/// An array-based trie that uses input sequence as keys
pub struct InputsTrie<T> {
    array: [Option<T>; usize::pow(5, INPUTS_CAP as u32)]
}

impl <T:Copy>InputsTrie<T> {
    pub const fn new() -> InputsTrie<T> {
        InputsTrie {
            array: [None; usize::pow(5, INPUTS_CAP as u32)]
        }
    }

    pub fn insert(&mut self, inputs: Inputs, ele: T) {
        self.array[Self::idx(&inputs)] = Some(ele);
    }

    pub fn get(&self, inputs: &[Input]) -> Option<T> {
        self.array[Self::idx(inputs)]
    }

    fn idx(inputs: &[Input]) -> usize {
        // cast the input sequence into a base-5 number
        const BASE: usize = 5;
        let mut idx = 0;
        for (i, input) in inputs.iter().cloned().take(INPUTS_CAP).enumerate() {
            idx += input.quinary_digit() * BASE.pow(i.try_into().unwrap());
        }
        idx
    }
}

