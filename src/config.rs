use std::{fs, path::Path};

use anyhow::Result;
use log::warn;
use crate::input::{Input::*, Inputs, InputsTrie};

pub struct Config{
    pub default_art: Option<u32>,
    pub arts: InputsTrie<u32>
}

impl Config {
    pub const fn new() -> Config {
        Config {
            default_art: None, // Wirlwind Slash
            arts: InputsTrie::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Config> {
        let mut config = Config::new();
        let mut file = fs::read_to_string(path)?;
        file.make_ascii_lowercase();

        for line in file.lines() {
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
            if !matches!(id, 5000..=10000) {
                warn!("Illegal combat art ID {id} is ignored.");
                continue;
            }

            
            if inputs.eq_ignore_ascii_case("none") || matches!(inputs, "∅" | "空")  {
                config.default_art = Some(id);
            } else {
                let inputs = inputs.chars()
                    .filter_map(|ch|match ch {
                        '↑'|'u'|'上' => Some(Up),
                        '↓'|'d'|'下' => Some(Down),
                        '←'|'l'|'左' => Some(Left),
                        '→'|'r'|'右' => Some(Right),
                        _ => None })
                    .take(3)
                    .collect::<Inputs>();
                // the last element of the line may not be the inputs but rather the name of the combat arts
                // parsing names as inputs can produce empty inputs
                if inputs.len() == 0 {
                    continue;
                }
                // fault tolerance for keyboards
                // example: if ←→ is used while →← is not, treat →← as ←→ so that players can press A and D at the same time
                if inputs.len() >= 2 {
                    let mut reversed = Inputs::new();
                    reversed.push(inputs[1]);
                    reversed.push(inputs[0]);
                }
                // fault tolerance for joysticks. 
                // example: if ↑↓ is used while ↑→↓ is not, treat ↑→↓ as ↑↓ so that players won't accidentally do semicircles
                if inputs.len() == 2 && inputs[0] == inputs[1].opposite() {
                    for fault in [Up, Right, Down, Left] {
                        if fault == inputs[0] || fault == inputs[1] {
                            continue;
                        }
                        let mut semicircle = Inputs::new();
                        semicircle.push(inputs[0]);
                        semicircle.push(fault);
                        semicircle.push(inputs[1]);
                        if config.arts.get(&semicircle).is_none() {
                            config.arts.insert(semicircle, id);
                        }
                    }
                }
                config.arts.insert(inputs, id);
            }
        }
        Ok(config)
    }
}