use std::{fs, path::Path};

use anyhow::Result;
use log::warn;
use crate::input::{Input::*, InputsTrie, Inputs};

pub struct Config{
    pub default_art: u32,
    pub arts: InputsTrie<u32>
}

impl Config {
    pub const fn new() -> Config {
        Config {
            default_art: 295, // Wirlwind Slash
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
            // TODO: compatability with other mods with new combat arts?
            if !matches!(id, 295|444|378|317|431|468|312|494|357|403|461|473|411|423|484|437|488) {
                warn!("Illegal combat art ID {id} is ignored.");
                continue;
            }
            
            if inputs.eq_ignore_ascii_case("default") {
                config.default_art = id;
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
                // Optimize the experience a bit
                if inputs.len() == 2 {
                    let mut reversed = Inputs::new();
                    reversed.push(inputs[1]);
                    reversed.push(inputs[0]);
                    if config.arts.get(&reversed).is_none() {
                        config.arts.insert(reversed, id);
                    }
                }
                config.arts.insert(inputs, id);
            }
        }
        Ok(config)
    }
}