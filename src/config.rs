use std::{collections::{HashMap, HashSet}, fs, io, path::Path};
use log::warn;
use crate::{core::UID, input::{Input::{self, *}, Inputs, InputsTrie}};

const COMBART_ART_UID_MIN: UID  = 5000;
const COMBART_ART_UID_MAX: UID  = 10000;
const PROSTHETIC_TOOL_UID_MIN: UID  = 70000;
const PROSTHETIC_TOOL_UID_MAX: UID  = 100000;

pub struct Config {
    pub arts: InputsTrie<UID>,
    pub tools: InputsTrie<&'static[UID]>,
    pub tools_for_block: &'static[UID],
    pub tools_on_m4: &'static[UID],
    pub tools_on_m5: &'static[UID],
}

impl Config {
    pub const fn new() -> Config {
        Config {
            arts: InputsTrie::new(),
            tools: InputsTrie::new(),
            tools_for_block: &[],
            tools_on_m4: &[],
            tools_on_m5: &[],
        }
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Config> {
        let file = fs::read_to_string(path)?.to_ascii_lowercase();
        Ok(file.into())
    }
}

impl<S: AsRef<str>> From<S> for Config {
    fn from(value: S) -> Config {
        let mut config = Config::new();
        let mut tools = HashMap::<Inputs, Vec<UID>>::new();
        let mut tools_for_block = Vec::new();
        let mut tools_for_m4 = Vec::new();
        let mut tools_for_m5 = Vec::new();
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
                    warn!("Illegal ID {id} is ignored."); 
                    continue;
                }
            };

            if tool {
                // tools to use when BLOCK is heled, usually umbrella
                match inputs {
                    "⛨" | "block" | "防" =>  tools_for_block.push(id),
                    "x1" | "m4" => tools_for_m4.push(id),
                    "x2" | "m5" => tools_for_m5.push(id),
                    other => if let Some(inputs) = parse_motion(other) {
                        used_inputs.insert(inputs.clone());
                        tools.entry(inputs.clone()).or_insert_with(Vec::new).push(id);
                    }
                }
            } else {
                if let Some(inputs) = parse_motion(inputs) {
                    used_inputs.insert(inputs.clone());
                    config.arts.insert(inputs.clone(), id);
                }
            }
        }

        // leak vecs into slices
        for (inputs, tools) in tools {
            config.tools.insert(inputs, tools.leak());
        }
        config.tools_for_block = tools_for_block.leak();
        config.tools_on_m4 = tools_for_m4.leak();
        config.tools_on_m5 = tools_for_m5.leak();

        // fault tolernce
        for inputs in used_inputs {
            for alt_inputs in possible_altenrnatives(&inputs) {
                if let Some(art) = config.arts.get(&inputs) {
                    config.arts.try_insert(alt_inputs.clone(), art);
                }
                if let Some(tools) = config.tools.get(&inputs) {
                    config.tools.try_insert(alt_inputs.clone(), tools);
                }
            }
        }
        config
    }
}


// reuturns the input represented by the string and its alternative form when fault tolerance is available
fn parse_motion(motion: &str) -> Option<Inputs> {
    if matches!(motion, "∅" | "空" | "none") {
        Some(Inputs::new())
    } else {
        let chars = motion.chars();
        let char_count = chars.count();
        let inputs = motion.trim().chars()
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
            return None
        }
        Some(inputs.into_iter().take(3).collect::<Inputs>())
    }
}

#[allow(unused)]
fn possible_altenrnatives(inputs: &[Input]) -> Vec<Inputs> {
    if inputs.len() == 2 {
        // fault tolerance for keyboards
        // example: if ←→ is used while →← is not, treat →← as ←→ so that players can press A and D at the same time
        let mut possible_inputs = Vec::new();
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
    } else if inputs == &[Lt, Dn, Rt] {
        vec![
            Inputs::from([Lt, Rt, Dn]),
            Inputs::from([Rt, Lt, Dn]),
            Inputs::from([Rt, Dn, Lt]),
            Inputs::from([Dn, Lt, Rt]),
            Inputs::from([Dn, Rt, Lt]),
        ]
    } else{
        Vec::new()
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

    assert_eq!(config.arts.get(&[]), Some(7100));
    assert_eq!(config.tools.get_or_default(&[]), &[70000, 70100]);

    assert_eq!(config.arts.get(&[Lt, Rt]), Some(5600));
    assert_eq!(config.arts.get(&[Rt, Lt]), Some(7200));

    assert_eq!(config.tools.get_or_default(&[Lt, Rt]), &[74000]);
    assert_eq!(config.tools.get_or_default(&[Rt, Lt]), &[74000]);
}
