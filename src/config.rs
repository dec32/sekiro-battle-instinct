use std::{fs, io, path::Path};
use const_default::ConstDefault;
use log::warn;
use crate::input::{Input::*, Inputs, InputsTrie};

pub struct Config {
    pub skills: InputsTrie<Skill>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Skill {
    pub art: Option<u32>,
    pub tool: Option<u32>
}

impl ConstDefault for Skill {
    const DEFAULT: Skill = Skill {
        art: None,
        tool: None
    };
}

impl Config {
    pub const fn new_const() -> Config {
        Config {
            skills: InputsTrie::new_const(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Config> {
        let file = fs::read_to_string(path)?.to_ascii_lowercase();
        Ok(file.into())
    }
}

impl<S: AsRef<str>> From<S> for Config {
    fn from(value: S) -> Config {
        let mut config = Config::new_const();
        for line in value.as_ref().lines() {
            if line.is_empty() || line.starts_with("#"){
                continue;
            }
            let mut split = line.split_whitespace();
            // between IDs and inputs there're names of combat arts. They're ignored here
            let Some(id) = split.next().and_then(|id|id.parse::<u32>().ok()) else {
                continue;
            };
            let Some(inputs) = split.last() else {
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
                let mut slot = config.skills.get(&inputs);
                if is_art {
                    slot.art = Some(id)
                } else {
                    slot.tool = Some(id)
                }
                config.skills.insert(inputs, slot);

                // alternative form, they cannot overwrite the configured ones
                for alt_inputs in possible_inputs {
                    let mut slot = config.skills.get(&alt_inputs);
                    if is_art {
                        slot.art.get_or_insert(id);
                    } else {
                        slot.tool.get_or_insert(id);
                    }
                    config.skills.insert(alt_inputs, slot);
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
    impl Skill {
        fn of(art: u32, tool: u32) -> Skill {
            Skill {
                art: Some(art).filter(|i|*i!=0),
                tool: Some(tool).filter(|i|*i!=0),
            }
        }
    }

    let raw = "
        5600  Floating Passage           ←→
        7200  Spiral Clound Passage      →←
        70000 Loaded Shuriken            ←→
        ";
    let config = Config::from(raw);
    let skills = config.skills;
    assert_eq!(skills.get(&[Lt, Rt]), Skill::of(5600, 70000));
    assert_eq!(skills.get(&[Rt, Lt]), Skill::of(7200, 70000));     // reversed for keyboard
    assert_eq!(skills.get(&[Lt, Up, Rt]), Skill::of(5600, 70000)); // semicircle for joystick
}
