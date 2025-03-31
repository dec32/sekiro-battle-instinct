use std::{env, fs::{self}, os::windows::fs::MetadataExt, path::PathBuf};
use chrono::Local;
use log::{self, Level, LevelFilter::*};
use anyhow::Result;

#[cfg(debug_assertions)]
const RELEASE: bool = false;
#[cfg(not(debug_assertions))]
const RELEASE: bool = true;

pub fn setup() -> Result<()> {
    let path = PathBuf::from(env::var("APPDATA")?).join("Sekiro");
    fs::create_dir_all(&path)?;
    let path = path.join("battle_instinct.log");
    if let Ok(meta) = fs::metadata(&path) {
        if meta.file_size() >= 5 * 1024 * 1024 {
            fs::remove_file(&path).ok();
        }
    }
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{:<3}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                record.level().abbr(),
                message
            ))
        })
        .level(if RELEASE { Warn } else { Debug })
        .chain(fern::log_file(path)?)
        .apply()?;
    std::panic::set_hook(Box::new(|info|log::error!("{info}")));
    Ok(())
}

trait LevelExt {
    fn abbr(self) -> &'static str;
}

impl LevelExt for Level {
    fn abbr(self) -> &'static str {
        match self {
            Level::Error => "ERR",
            Level::Warn  => "WRN",
            Level::Info  => "INF",
            Level::Debug => "DBG",
            Level::Trace => "TRC",
        }
    }
}
