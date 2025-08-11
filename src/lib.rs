mod config;
mod core;
mod frame;
mod game;
mod input;
mod logger;

use core::Mod;
use std::{
    ffi::{OsStr, OsString, c_void},
    fs, mem,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread::{self},
    time::Duration,
};

use anyhow::{Result, anyhow};
use frame::FRAMERATE;
use minhook::MinHook;
use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE},
        System::{
            LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW},
            SystemInformation::GetSystemDirectoryW,
            SystemServices::DLL_PROCESS_ATTACH,
        },
    },
    core::{GUID, HRESULT, PCWSTR, s},
};

//----------------------------------------------------------------------------
//
//  Entry for the DLL
//
//----------------------------------------------------------------------------

#[unsafe(no_mangle)]
#[allow(non_snake_case, dead_code)]
extern "stdcall" fn DllMain(dll_module: HINSTANCE, call_reason: u32, _reserved: *mut ()) -> bool {
    if call_reason == DLL_PROCESS_ATTACH {
        logger::setup().ok();
        let mut buf: Vec<u16> = vec![0; 128];
        let len = unsafe { GetModuleFileNameW(dll_module, buf.as_mut_slice()) } as usize;
        let dll_path = PathBuf::from(OsString::from_wide(&buf[..len]));
        let dir_path = dll_path.parent().unwrap();
        chainload(dir_path);
        modify(dir_path);
    }
    true
}

//----------------------------------------------------------------------------
//
//  Redirect DirectInput8Create to the original dinput8.dll
//
//----------------------------------------------------------------------------

#[unsafe(no_mangle)]
extern "stdcall" fn DirectInput8Create(
    hinst: HINSTANCE,
    dwversion: u32,
    riidltf: *const GUID,
    ppvout: *mut *mut c_void,
    punkouter: HINSTANCE,
) -> HRESULT {
    match load_dll() {
        Ok(proc) => proc(hinst, dwversion, riidltf, ppvout, punkouter),
        Err(e) => e.into(),
    }
}

fn load_dll() -> windows::core::Result<fn(HINSTANCE, u32, *const GUID, *mut *mut c_void, HINSTANCE) -> HRESULT> {
    unsafe {
        let mut path = vec![0; 128];
        let len = GetSystemDirectoryW(Some(&mut path));
        path.truncate(len as usize);
        path.extend(OsStr::new("\\dinput8.dll\0").encode_wide());
        let hmodule = LoadLibraryW(PCWSTR::from_raw(path.as_ptr()))?;
        let Some(address) = GetProcAddress(hmodule, s!("DirectInput8Create")) else {
            return Err(GetLastError().into());
        };
        let address = address as usize;
        let path = OsString::from_wide(&path[..path.len() - 1]).into_string().unwrap();
        log::debug!("Located DirectInput8Create at {:#08x}({}).", address, path);
        Ok(mem::transmute(address))
    }
}

//----------------------------------------------------------------------------
//
//  Chainload other dinput8.dll files used by other MODs
//
//----------------------------------------------------------------------------

fn chainload(path: &Path) {
    _chainload(path)
        .inspect_err(|e| log::error!("Failed to chainload other dinput8.dll files. {e}"))
        .ok();
}

fn _chainload(path: &Path) -> Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path)?.filter_map(Result::ok) {
        let name = entry.file_name();
        let name_lossy = name.to_string_lossy();
        // We really needs an STD regex lib
        if !name_lossy.starts_with("dinput8_") {
            continue;
        }
        if !name_lossy.ends_with(".dll") {
            continue;
        }
        names.push(name);
    }
    // Load the DLL by the order of names so that players can use names like
    // dinput8_1_xxx.dll, dinput8_2_xxx.dll to determine chainload order
    names.sort();
    for name in names {
        let path = path.join(&name);
        let path = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            LoadLibraryW(PCWSTR::from_raw(path.as_ptr()))?;
        }
        log::debug!("Chainloaded dll: {name:?}");
    }
    Ok(())
}

//----------------------------------------------------------------------------
//
//  Initialize the MOD
//
//----------------------------------------------------------------------------

const HOOK_DELAY: Duration = Duration::from_secs(10);
static MOD: Mutex<Mod> = Mutex::new(Mod::new());
static PROCESS_INPUT_ORIG: OnceLock<fn(*mut game::InputHandler, usize) -> usize> = OnceLock::new();

fn modify(path: &Path) {
    let path = path.join("battle_instinct.cfg");
    thread::spawn(|| {
        // hooking fails if it starts too soon (MH_ERROR_UNSUPPORTED_FUNCTION)
        thread::sleep(HOOK_DELAY);
        _modify(path).inspect_err(|e| log::error!("Errored occured when modulating. {e}"))
    });
}

fn _modify(path: PathBuf) -> Result<()> {
    MOD.lock().unwrap().load_config(&path)?;
    unsafe {
        let process_input_orig = MinHook::create_hook(game::PROCESS_INPUT as *mut c_void, process_input as *mut c_void)
            .map_err(|e| anyhow!("{e:?}"))?;
        PROCESS_INPUT_ORIG.set(mem::transmute(process_input_orig)).unwrap();
        MinHook::enable_all_hooks().map_err(|e| anyhow!("{e:?}"))?;
    }
    Ok(())
}

fn process_input(input_handler: *mut game::InputHandler, arg: usize) -> usize {
    unsafe {
        FRAMERATE.tick();
    }
    MOD.lock()
        .unwrap()
        .process_input(unsafe { input_handler.as_mut().expect("input_handler is null") });
    let process_input_orig = PROCESS_INPUT_ORIG.get().copied().unwrap();
    process_input_orig(input_handler, arg)
}
