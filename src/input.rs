use arrayvec::ArrayVec;
use log::debug;



pub const INPUTS_CAP: usize = 3;
const MAX_AGE: u8 = 10;

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
            1 => Input::Up,
            2 => Input::Down,
            3 => Input::Left,
            4 => Input::Right,
            _ => panic!("You idiot you shouldn't mess up such simple thing.")
        }
    }
}


/// A stack-allocated container for input sequences
pub type Inputs = ArrayVec<Input, INPUTS_CAP>;


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

    pub fn update<'s>(&'s mut self, up: bool, down: bool, left: bool, right: bool) -> Inputs {
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
            debug!("Buffer: {:?}", self.inputs);
            self.age = 0
        } else {
            self.age = u8::min(self.age + 1, MAX_AGE)
        }
        self.inputs.clone()
    }

    pub fn aborted(&self) -> bool {
        self.holds == [false, false, false, false] && self.age >= MAX_AGE
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

