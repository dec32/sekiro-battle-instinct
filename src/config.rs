use std::{collections::{HashMap, HashSet}, fs, io, path::Path};
use widestring::U16CStr;
use crate::{core::UID, game, input::{Input::*, Inputs, InputsTrie}};

const COMBART_ART_UID_MIN: UID  = 5000;
const COMBART_ART_UID_MAX: UID  = 10000;
const PROSTHETIC_TOOL_UID_MIN: UID  = 70000;
const PROSTHETIC_TOOL_UID_MAX: UID  = 100000;

#[derive(Debug)]
pub struct Config {
    pub arts: InputsTrie<UID>,
    pub tools: InputsTrie<&'static[UID]>,
    pub tools_for_block: &'static[UID],
    pub tools_on_x1: &'static[UID],
    pub tools_on_x2: &'static[UID],
}

impl Config {
    pub const fn new() -> Config {
        Config {
            arts: InputsTrie::new(),
            tools: InputsTrie::new(),
            tools_for_block: &[],
            tools_on_x1: &[],
            tools_on_x2: &[],
        }
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Config> {
        let file = fs::read_to_string(path)?.to_ascii_uppercase();
        Ok(file.into())
    }
}

impl<S: AsRef<str>> From<S> for Config {
    fn from(value: S) -> Config {
        let mut config = Config::new();
        let mut tools = HashMap::<Inputs, Vec<UID>>::new();
        let mut tools_for_block = Vec::new();
        let mut tools_on_x1 = Vec::new();
        let mut tools_on_x2 = Vec::new();
        let mut used_inputs = HashSet::new();
        for line in value.as_ref().lines() {
            let mut items = line.split_whitespace()
                .take_while(|item|!item.starts_with("#"));
            // between IDs and inputs there're names of combat arts. They're ignored here
            let Some(id) = items.next().and_then(|id|id.parse::<UID>().ok()) else {
                continue;
            };
            let Some(inputs) = items.last() else {
                continue;
            };
            // filter out all illegal IDs to prevent possible bugs
            let tool = match id {
                PROSTHETIC_TOOL_UID_MIN..=PROSTHETIC_TOOL_UID_MAX => true,
                COMBART_ART_UID_MIN..=COMBART_ART_UID_MAX => false,
                _ => {
                    log::warn!("Illegal ID {id} is ignored."); 
                    continue;
                }
            };

            if tool {
                // tools to use when BLOCK is heled, usually umbrella
                match inputs {
                    "X1"|"M4" => tools_on_x1.push(id),
                    "X2"|"M5" => tools_on_x2.push(id),
                    "⛉" | "BLOCK" => tools_for_block.push(id),
                    other => if let Some(inputs) = parse_motion(other) {
                        used_inputs.insert(inputs);
                        tools.entry(inputs).or_insert_with(Vec::new).push(id);
                    }
                }
            } else {
                if let Some(inputs) = parse_motion(inputs) {
                    used_inputs.insert(inputs);
                    config.arts.insert(inputs, id);
                }
            }
        }

        // leak vecs into slices
        for (inputs, tools) in tools {
            config.tools.insert(inputs, tools.leak());
        }
        config.tools_for_block = tools_for_block.leak();
        config.tools_on_x1 = tools_on_x1.leak();
        config.tools_on_x2 = tools_on_x2.leak();
        
        // fault tolernce
        for inputs in used_inputs {
            for alt_inputs in possible_altenrnatives(inputs) {
                if let Some(art) = config.arts.get(inputs) {
                    config.arts.try_insert(alt_inputs, art);
                }
                if let Some(tools) = config.tools.get(inputs) {
                    config.tools.try_insert(alt_inputs, tools);
                }
            }
        }
        config
    }
}


// reuturns the input represented by the string and its alternative form when fault tolerance is available
fn parse_motion(motion: &str) -> Option<Inputs> {
    if matches!(motion, "∅" | "NONE") {
        Some(Inputs::new())
    } else {
        let chars = motion.chars();
        let char_count = chars.count();
        let inputs = motion.trim().chars()
            .filter_map(|ch|ch.try_into().ok())
            .collect::<Vec<_>>();
        // the last element of the line may not be the inputs but rather the name of the combat arts
        if inputs.len() != char_count {
            return None
        }
        Some(inputs.into_iter().take(3).collect::<Inputs>())
    }
}

#[allow(unused)]
fn possible_altenrnatives(mut inputs: Inputs) -> Vec<Inputs> {
    if inputs.len() == 2 {
        // fault tolerance for keyboards
        // example: if ←→ is used while →← is not, treat →← as ←→ so that players can press A and D at the same time
        let mut possible_inputs = Vec::new();
        possible_inputs.push(inputs.rev());
        let a = inputs.pop().unwrap();
        let b = inputs.pop().unwrap();
        if a == b {
            // button smash
            possible_inputs.push(Inputs::from([a, a, a]));
        } else if a == b.opposite() {
            // semicircle, for gamepads
            possible_inputs.push(Inputs::from([a, a.rotate(), b]));
            possible_inputs.push(Inputs::from([a, b.rotate(), b]));
        }
        possible_inputs
    } else if inputs == [Left, Down, Right].into() {
        vec![
            Inputs::from([Left, Right, Down]),
            Inputs::from([Right, Left, Down]),
            Inputs::from([Right, Down, Left]),
            Inputs::from([Down, Left, Right]),
            Inputs::from([Down, Right, Left]),
        ]
    } else {
        Vec::new()
    }
}


#[allow(unused)]
fn get_item_name(uid: UID) -> Option<String> {
    let p = game::get_item_name(game::msg_repo(), uid);
    if p.is_null() {
        return None;
    } else {
        let name = unsafe { U16CStr::from_ptr_str(p) };
        Some(name.to_string_lossy())
    }
}

#[test]
fn test_load() {
    let raw = "
        # this is a line of comment
        7100  Ichimonji: Double           ∅  # comment
        70000 Loaded Shuriken             ∅  # comment
        70100 Spinnging Shuriken          ∅  # comment
        5600  Floating Passage           ←→  # comment
        7200  Spiral Clound Passage      →←  # comment
        74000 Mist Raven                 ←→  # comment
        ";
    let config = Config::from(raw);

    assert_eq!(config.arts.get([]), Some(7100));
    assert_eq!(config.tools.get_or_default([]), [70000, 70100]);

    assert_eq!(config.arts.get([Left, Right]), Some(5600));
    assert_eq!(config.arts.get([Right, Left]), Some(7200));

    assert_eq!(config.tools.get_or_default([Left, Right]), &[74000]);
    assert_eq!(config.tools.get_or_default([Right, Left]), &[74000]);
}
