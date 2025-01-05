use arrayvec::ArrayVec;
use log::trace;
use Input::*;

const INPUTS_CAP: usize = 3;
const BUFFER_MAX_AGE: u8 = 10;
const JOYSTICK_THRESHOLD: i16 = i16::MAX / 100 * 80;

/// I love type safety and readability.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Input {
    Up = 1, 
    Down = 2, 
    Left = 3, 
    Right = 4 
}

impl From<usize> for Input {
    fn from(value: usize) -> Self {
        match value {
            1 => Up,
            2 => Down,
            3 => Left,
            4 => Right,
            _ => panic!("You idiot you shouldn't mess up such simple thing.")
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
    holds: [bool; 4],
    age: u8,
}

impl InputBuffer {
    pub const fn new() -> InputBuffer {
        InputBuffer {
            inputs: Inputs::new_const(),
            holds: [false; 4],
            age: 0
        }
    }

    // TODO it should NOT expose outdated buffer to the outside
    pub fn update(&mut self, up: bool, down: bool, left: bool, right: bool) -> Inputs {
        let mut updated = false;
        for (i, hold) in [up, down, left, right].iter().cloned().enumerate() {
            if !self.holds[i] && hold {
                // newly pressed direction
                if self.inputs.len() >= self.inputs.capacity() || self.aborted() {
                    self.inputs.clear();
                }
                self.inputs.push(Input::from(i + 1));
                updated = true;
            }
            self.holds[i] = hold;
        }

        if updated {
            self.age = 0;
            trace!("Buffer: {:?}", self.inputs);
        } else {
            self.age = u8::min(self.age + 1, BUFFER_MAX_AGE);
        }
        self.inputs.clone()
    }

    pub fn update_pos(&mut self, x: i16, y: i16) -> Inputs {
        let distance_square = (x as i32).pow(2) + (y as i32).pow(2);
        let threshold_square = (JOYSTICK_THRESHOLD as i32).pow(2);
        if distance_square < threshold_square {
            // TODO 检测摇杆是否回到「中立区」时，选择一个偏移过的原点充当距离计算的基准
            // 缓冲区清空时，再把原点重置
            self.update(false, false, false, false)
        } else {
            let dir = if y.unsigned_abs() >= x.unsigned_abs() {
                if y > 0 { Up } else { Down }
            } else {
                if x > 0 { Right } else { Left } 
            };
            match dir {
                Up => self.update(true, false, false, false),
                Down => self.update(false, true, false, false),
                Left => self.update(false, false, true, false),
                Right => self.update(false, false, false, true),
            }
        }
    }


    pub fn aborted(&self) -> bool {
        self.holds == [false, false, false, false] && self.age >= BUFFER_MAX_AGE
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
        let mut idx = 0;
        for (i, input) in inputs.iter().cloned().take(INPUTS_CAP).enumerate() {
            idx += input as usize * usize::pow(5, i as u32);
        }
        idx
    }
}

