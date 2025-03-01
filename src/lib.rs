mod core;
mod game;
mod config;
mod input;
mod frame;
mod logging;

use core::Mod;
use std::{ffi::{c_void, OsStr, OsString}, fs, mem, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, sync::{Mutex, OnceLock}, thread, time::Duration};
use anyhow::{anyhow, Result};
use minhook::MinHook;
use windows::{core::{s, GUID, HRESULT, PCWSTR}, Win32::{Foundation::{GetLastError, HINSTANCE}, System::{LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW}, SystemInformation::GetSystemDirectoryW, SystemServices::DLL_PROCESS_ATTACH}}};


//----------------------------------------------------------------------------
//
//  Entry for the DLL
//
//----------------------------------------------------------------------------

#[no_mangle]
#[allow(non_snake_case, dead_code)]
extern "stdcall" fn DllMain(dll_module: HINSTANCE, call_reason: u32, _reserved: *mut()) -> bool {
    if call_reason == DLL_PROCESS_ATTACH {
        logging::setup().ok();
        let mut buf: Vec<u16> = vec![0;128];
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

#[no_mangle]
#[allow(non_snake_case, dead_code)]
extern "stdcall" fn DirectInput8Create(hinst: HINSTANCE, dwversion: u32, riidltf: *const GUID, ppvout: *mut *mut c_void, punkouter: HINSTANCE) -> HRESULT {
    match load_dll() {
        Ok(address) => {
            let f = unsafe { mem::transmute::<_, fn(HINSTANCE, u32, *const GUID, *mut *mut c_void, HINSTANCE)->HRESULT>(address) };
            f(hinst, dwversion, riidltf, ppvout, punkouter)
        },
        Err(e) => e.into()
    }
}


fn load_dll() -> windows::core::Result<usize> {
    let mut path = vec![0;128];
    unsafe {
        let len = GetSystemDirectoryW(Some(&mut path));
        path.truncate(len as usize);
        path.extend_from_slice(OsStr::new("\\dinput8.dll\0").encode_wide().collect::<Vec<_>>().as_slice());
        let hmodule = LoadLibraryW(PCWSTR::from_raw(path.as_ptr()))?;
        let Some(address) = GetProcAddress(hmodule, s!("DirectInput8Create")) else {
            return Err(GetLastError().into());
        };
        let address = mem::transmute::<_, usize>(address);
        let path = OsString::from_wide(&path[..path.len() - 1]).into_string().unwrap();
        log::debug!("Located DirectInput8Create at {:#08x}({}).", address, path);
        Ok(address)
    }
}


//----------------------------------------------------------------------------
//
//  Chainload other dinput8.dll files used by other MODs
//
//----------------------------------------------------------------------------

fn chainload(path: &Path) {
    _chainload(path).inspect_err(|e|log::error!("Failed to chainload other dinput8.dll files. {e}")).ok();
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
static MOD: Mutex<Option<Mod>> = Mutex::new(Some(Mod::new_const()));
static PROCESS_INPUT_ORIG: OnceLock<fn(*mut game::InputHandler, usize) -> usize> = OnceLock::new();

fn modify(path: &Path) {
    let path = path.join("battle_instinct.cfg");
    thread::spawn(||{
        // hooking fails if it starts too soon (MH_ERROR_UNSUPPORTED_FUNCTION)
        thread::sleep(HOOK_DELAY);
        _modify(path).inspect_err(|e|log::error!("Errored occured when modulating. {e}"))
    });
}

fn _modify(path: PathBuf) -> Result<()> {
    MOD.lock().unwrap().as_mut().unwrap().load_config(&path)?;
    unsafe {
        let process_input_orig = MinHook::create_hook(
            game::PROCESS_INPUT as *mut c_void,
            process_input as *mut c_void).map_err(|e|anyhow!("{e:?}"))?;
        PROCESS_INPUT_ORIG.set(mem::transmute(process_input_orig)).unwrap();
        MinHook::enable_all_hooks().map_err(|e|anyhow!("{e:?}"))?;
    }
    Ok(())
}

fn process_input(input_handler: *mut game::InputHandler, arg: usize) -> usize {
    let mut guard = MOD.lock().unwrap();
    if let Some(_mod) = guard.as_mut() {
        match _mod.process_input(input_handler) {
            Ok(_) => (),
            Err(e) => {
                log::error!("{e}");
                guard.take();
            }
        }
    }
    let process_input_orig = PROCESS_INPUT_ORIG.get().cloned().unwrap();
    process_input_orig(input_handler, arg)
}


