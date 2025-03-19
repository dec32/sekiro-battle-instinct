use std::{collections::{HashMap, HashSet}, fs, io, path::Path};
use log::warn;
use crate::input::{Input::{self, *}, Inputs, InputsTrie};

pub struct Config {
    arts: InputsTrie<u32>,
    tools: InputsTrie<&'static[u32]>,
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

    pub fn get_art(&self, inputs: &[Input]) -> Option<u32> {
        self.arts.get(inputs)
    }

    pub fn get_default_art(&self) -> Option<u32> {
        self.arts.get(&[])
    }

    pub fn get_tools(&self, inputs: &[Input]) -> &'static[u32] {
        self.tools.get_or_default(inputs)
    }

    pub fn get_default_tools(&self) -> &'static[u32] {
        self.tools.get_or_default(&[])
    }
}

impl<S: AsRef<str>> From<S> for Config {
    fn from(value: S) -> Config {
        let mut config = Config::new_const();
        let mut tools_map = HashMap::<Inputs, Vec<u32>>::new();
        let mut inputs_set = HashSet::new();
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

            let Some(inputs) = parse_inputs(inputs) else {
                continue;
            };
            inputs_set.insert(inputs.clone());

            if is_art {
                config.arts.insert(inputs.clone(), id);
            } else {
                tools_map.entry(inputs).or_insert_with(Vec::new).push(id);
            }
        }
        for (inputs, tools) in tools_map {
            config.tools.insert(inputs, tools.leak());
        }

        // fault tolernce
        for inputs in inputs_set {
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
fn parse_inputs(inputs: &str) -> Option<Inputs> {
    if matches!(inputs, "∅" | "空" | "none") {
        Some(Inputs::new())
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
            return None
        }
        Some(inputs.into_iter().take(3).collect::<Inputs>())
    }
}

#[allow(unused)]
fn possible_altenrnatives(inputs: &[Input]) -> Vec<Inputs> {
    if inputs.len() >= 2 {
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
    } else {
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

    assert_eq!(config.get_default_art(), Some(7100));
    assert_eq!(config.get_default_tools(), &[70000, 70100]);

    assert_eq!(config.get_art(&[Lt, Rt]), Some(5600));
    assert_eq!(config.get_art(&[Rt, Lt]), Some(7200));

    assert_eq!(config.get_tools(&[Lt, Rt]), &[74000]);
    assert_eq!(config.get_tools(&[Rt, Lt]), &[74000]);

}
