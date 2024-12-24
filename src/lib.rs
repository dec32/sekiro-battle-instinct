mod log;
mod input;
mod config;

use std::{ffi::{OsStr, OsString}, mem, os::{raw::c_void, windows::ffi::{OsStrExt, OsStringExt}}, path::PathBuf, thread, time::Duration};
use anyhow::Result;
use input::InputBuffer;
use minhook::MinHook;
use config::Config;
use windows::{core::{s, GUID, HRESULT, PCWSTR}, Win32::{Foundation::{GetLastError, HINSTANCE}, System::{LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW}, SystemInformation::GetSystemDirectoryW, SystemServices::DLL_PROCESS_ATTACH}, UI::Input::KeyboardAndMouse::GetKeyState}};
use ::log::{debug, error};


//----------------------------------------------------------------------------
//
//  Entry for the DLL
//
//----------------------------------------------------------------------------

#[no_mangle]
#[allow(non_snake_case, dead_code)]
extern "stdcall" fn DllMain(dll_module: HINSTANCE, call_reason: u32, _reserved: *mut()) -> bool {
    if call_reason == DLL_PROCESS_ATTACH {
        log::setup();
        let mut path = vec![0;128];
        let len = unsafe { GetModuleFileNameW(dll_module, path.as_mut_slice()) } as usize;
        let dll_path = PathBuf::from(OsString::from_wide(&path[..len]));
        thread::spawn(||{
            modulate(dll_path).inspect_err(|e|error!("Errored occured when initializing. {e}")).ok();
        });
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


fn load_dll() -> windows::core::Result<usize>{
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
        debug!("Located DirectInput8Create at {:#08x}({}).", address, path);
        Ok(address)
    }
}


//----------------------------------------------------------------------------
//
//  Actual content of the mod
//
//----------------------------------------------------------------------------

static mut PROCESS_INPUT: usize = 0x140B2C190;
static mut CONFIG: Config = Config::new();

fn modulate(mut path: PathBuf) -> Result<()> {
    // hooking will fail (MH_ERROR_UNSUPPORTED_FUNCTION) if it starts too soon
    thread::sleep(Duration::from_secs(30));
    unsafe {
        // Loading configs
        path.pop();
        path.push("battle_instinct.cfg");
        CONFIG = Config::load(&path)?;
        // TODO ? operator doesn't work on MinHook
        PROCESS_INPUT = MinHook::create_hook(PROCESS_INPUT as *mut c_void, process_input as *mut c_void).unwrap() as usize; 
        MinHook::enable_all_hooks().unwrap();
    }
    Ok(())
}


// Some unholy static mut to track states
static mut BUFFER: InputBuffer = InputBuffer::new();
static mut CUR_COMBAT_ART: u32 = 0;
static mut BLOCKING_LAST_FRAME: bool = false;

fn process_input(input_handler: usize, arg: usize) -> usize {    
    unsafe fn is_key_down(keycode: i32) -> bool {
        GetKeyState(keycode) as u16 & 0x8000 != 0
    }

    unsafe {
        let blocking_now = is_key_down(0x02);
        let desired_combat_art = if !BLOCKING_LAST_FRAME && blocking_now && BUFFER.aborted() {
            // roll back to the default combat arts when block is pressed if there're no recent inputs
            // TODO 长按右键释放完一个武技后也要能回退到默认武技
            BUFFER.clear();
            CONFIG.default_art
        } else {
            // otherwise let's keep keeping track recent diretional inputs
            let up = is_key_down(0x57);
            let down = is_key_down(0x53);
            let left = is_key_down(0x41);
            let right = is_key_down(0x44);
            let inputs = BUFFER.update(up, down, left, right);
            CONFIG.arts.get(&inputs).unwrap_or(CONFIG.default_art)
        };
        
        if desired_combat_art != CUR_COMBAT_ART {
            set_combat_art(desired_combat_art);
            CUR_COMBAT_ART = desired_combat_art;
        }
        BLOCKING_LAST_FRAME = blocking_now;
    }
    let f = unsafe{ mem::transmute::<_, fn(usize, usize)->usize>(PROCESS_INPUT) };
    f(input_handler, arg)
}



//----------------------------------------------------------------------------
//
//  called functions (derive macro)
//
//----------------------------------------------------------------------------


// TODO use some derive macro to clean up this mess
const SET_SKILL_SLOT: usize = 0x140D592F0;
const PLAY_UI_SOUND: usize = 0x1408CE960;


fn set_skill_slot(equip_slot: isize, data_pointer: *const u32, ignore_equip_lock: bool) {
    // equip_slot: 1 represents the combat art slot. 0, 2 and 4 represents the prosthetic slots
    // data_pointer: data_pointer[14] is for combat art ID. data_pointer[16] is for prosthetics ID
    let f = unsafe{ mem::transmute::<_, fn(isize, *const u32 ,bool)>(SET_SKILL_SLOT) };
    f(equip_slot, data_pointer, ignore_equip_lock);
}

#[allow(dead_code)]
fn play_ui_sound(arg1: isize, arg2: isize) {
    let f = unsafe{ mem::transmute::<_, fn(isize, isize)>(PLAY_UI_SOUND) };
    f(arg1, arg2);
}



//----------------------------------------------------------------------------
//
//  wrapper for called functions (derive macro)
//
//----------------------------------------------------------------------------

fn set_combat_art(id: u32) {
    // cast the combat art id to some sort of "equip data" array.
    static mut EQUIP_DATA: [u32;17] = [0;17];
    // equip the same combat art again will drop the combat art (sometimes?)
    let data_pointer = unsafe {
        EQUIP_DATA[14] = id;
        EQUIP_DATA.as_ptr()
    };
    set_skill_slot(1, data_pointer, true);
}