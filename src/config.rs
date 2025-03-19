use std::{fs, io, path::Path};
use log::warn;
use crate::input::{Input::*, Inputs, InputsTrie};

pub struct Config {
    arts: InputsTrie<u32>,
    tools: InputsTrie<u32>,
}

impl Config {
    pub const fn new_const() -> Config {
        Config {
            arts: InputsTrie::new_const(),
            tools: InputsTrie::new_const()
        }
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Config> {
        let file = fs::read_to_string(path)?.to_ascii_lowercase();
        Ok(file.into())
    }

    pub fn get_art(&self, inputs: &Inputs) -> Option<u32> {
        self.arts.get(inputs)
    }

    pub fn get_default_art(&self) -> Option<u32> {
        self.arts.get(&[])
    }

    pub fn get_tool(&self, inputs: &Inputs) -> Option<u32> {
        self.tools.get(inputs)
    }

    pub fn get_default_tool(&self) -> Option<u32> {
        self.tools.get(&[])
    }
}

impl<S: AsRef<str>> From<S> for Config {
    fn from(value: S) -> Config {
        let mut config = Config::new_const();
        for line in value.as_ref().lines() {
            let mut items = line.split_whitespace()
                .take_while(|item|!item.starts_with("#"));
            // between IDs and inputs there're names of combat arts. They're ignored here
            let Some(id) = items.next().and_then(|id|id.parse::<u32>().ok()) else {
                continue;
            };
            let Some(inputs) = items.last() else {
                continue;
            };

            // filter out all illegal IDs to prevent possible bugs
            let is_art = match id {
                5000..=10000 => true,
                70000..=100000 => false,
                _ => {
                    warn!("Illegal ID {id} is ignored."); 
                    continue;
                }
            };

            let mut possible_inputs = parse_possible_inputs(inputs).into_iter();
            if let Some(inputs) = possible_inputs.next() {
                // the configured inputs
                if is_art {
                    config.arts.insert(inputs.clone(), id);
                } else {
                    config.tools.insert(inputs, id);
                }
                
                // alternative form, they cannot overwrite the configured ones
                for alt_inputs in possible_inputs {
                    if is_art {
                        config.arts.try_insert(alt_inputs.clone(), id);
                    } else {
                        config.tools.try_insert(alt_inputs, id);
                    }
                }
            }
        }
        config
    }
}

// reuturns the input represented by the string and its alternative form when fault tolerance is available
fn parse_possible_inputs(inputs: &str) -> Vec<Inputs> {
    if matches!(inputs, "∅" | "空" | "none") {
        vec![Inputs::new()]
    } else {
        let chars = inputs.chars();
        let char_count = chars.count();
        let inputs = inputs.trim().chars()
            .filter_map(|ch|match ch {
                '↑'|'8'|'u'|'上' => Some(Up),
                '→'|'6'|'r'|'右' => Some(Rt),
                '↓'|'2'|'d'|'下' => Some(Dn),
                '←'|'4'|'l'|'左' => Some(Lt),
                '↗'|'9' => Some(Ur),
                '↘'|'3' => Some(Dr),
                '↙'|'1' => Some(Dl),
                '↖'|'7' => Some(Ul),
                _ => None,
            })
            .collect::<Vec<_>>();
        // the last element of the line may not be the inputs but rather the name of the combat arts
        if inputs.len() != char_count {
            return vec![]
        }

        let inputs = inputs.into_iter().take(3).collect::<Inputs>();
        if inputs.len() >= 2 {
            // fault tolerance for keyboards
            // example: if ←→ is used while →← is not, treat →← as ←→ so that players can press A and D at the same time
            let mut possible_inputs = vec![inputs.clone()];
            let mut rev = Inputs::new();
            rev.push(inputs[1]);
            rev.push(inputs[0]);
            possible_inputs.push(rev);
            if inputs[0] == inputs[1].opposite() {
                for fault in [Up, Rt, Dn, Lt] {
                    if fault == inputs[0] || fault == inputs[1] {
                        continue;
                    }
                    let mut semicircle = Inputs::new();
                    semicircle.push(inputs[0]);
                    semicircle.push(fault);
                    semicircle.push(inputs[1]);
                    possible_inputs.push(semicircle);
                }
            }
            possible_inputs
        } else {
            vec![inputs]
        }
    }
}

#[test]
fn test_load() {

    // let raw = "
    //     # this is a line of comment
    //     7100  Ichimonji: Double           ∅  # comment
    //     70000 Loaded Shuriken             ∅  # comment
    //     5600  Floating Passage           ←→  # comment
    //     7200  Spiral Clound Passage      →←  # comment
    //     74000 Mist Raven                 ←→  # comment
    //     ";
    // let config = Config::from(raw);
}
