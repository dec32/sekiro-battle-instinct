mod log;
mod input;
mod config;

use std::{ffi::{c_void, OsStr, OsString}, fs, mem, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, thread, time::Duration};
use anyhow::Result;
use input::{InputBuffer, InputsExt};
use minhook::MinHook;
use config::Config;
use windows::{core::{s, GUID, HRESULT, PCWSTR}, Win32::{Foundation::{GetLastError, HINSTANCE}, System::{LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW}, SystemInformation::GetSystemDirectoryW, SystemServices::DLL_PROCESS_ATTACH}, UI::Input::KeyboardAndMouse::GetKeyState}};
use ::log::{debug, error};


//----------------------------------------------------------------------------
//
//  Some basic constants
//
//----------------------------------------------------------------------------


// Combat art UIDs
const ICHIMONJI: u32 = 5300;
const ICHIMONJI_DOUBLE: u32 = 7100;

const PRAYING_STRIKES: u32 = 5900;
const PRAYING_STRIKES_EXORCISM: u32 = 7500;

const SENPO_LEAPING_KICKS: u32 = 5800;
const HIGH_MONK: u32 = 7400;

const SHADOWRUSH: u32 = 6000;
const SHADOWFALL: u32 = 7600;

const MORTAL_DRAW: u32 = 5700;
const EMPOWERED_MORTAL_DRAW: u32 = 7300;

// Action bitfields
const ATTACK: u64 = 1;
const BLOCK: u64 = 4;
#[allow(unused)]
const JUMP: u64 = 16;
#[allow(unused)]
const SWITCH_PROSTHETIC: u64 = 1024;
#[allow(unused)]
const DODGE: u64 = 8192;
#[allow(unused)]
const USE_PROSTHETIC: u64 = 1074003970;

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
        thread::spawn(move ||{
            let dir_path = dll_path.parent().unwrap();
            chainload(dir_path).inspect_err(|e|error!("Failed to chainload other dinput8.dll files. {e}")).ok();
            modulate(dir_path).inspect_err(|e|error!("Errored occured when initializing. {e}")).ok();
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
        debug!("Located DirectInput8Create at {:#08x}({}).", address, path);
        Ok(address)
    }
}


//----------------------------------------------------------------------------
//
//  Chainload other dinput8.dll files used by other MODs
//
//----------------------------------------------------------------------------

fn chainload(path: &Path) -> Result<()> {
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
        // Load the DLL
        let path = path.join(&name);
        let path = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            LoadLibraryW(PCWSTR::from_raw(path.as_ptr()))?;
        }
        debug!("Chainloaded dll: {name_lossy}");
    }
    Ok(())
}



//----------------------------------------------------------------------------
//
//  Actual content of the mod
//
//----------------------------------------------------------------------------

static mut PROCESS_INPUT: usize = 0x140B2C190;
static mut CONFIG: Config = Config::new();

fn modulate(path: &Path) -> Result<()> {
    // hooking will fail (MH_ERROR_UNSUPPORTED_FUNCTION) if it starts too soon
    thread::sleep(Duration::from_secs(30));
    unsafe {
        // Loading configs
        let path = path.join("battle_instinct.cfg");
        CONFIG = Config::load(&path)?;
        // TODO ? operator doesn't work on MinHook
        // Hijack the input processing function
        PROCESS_INPUT = MinHook::create_hook(PROCESS_INPUT as *mut c_void, process_input as *mut c_void).unwrap() as usize; 
        MinHook::enable_all_hooks().unwrap();
    }
    Ok(())
}


fn process_input(input_handler: *const c_void, arg: usize) -> usize {
    // Some unholy static mut to track states
    static mut BUFFER: InputBuffer = InputBuffer::new();
    static mut BLOCKING_LAST_FRAME: bool = false;
    
    unsafe fn is_key_down(keycode: i32) -> bool {
        GetKeyState(keycode) as u16 & 0x8000 != 0
    }

    unsafe {
        // If you forget what a bitfield is please refer to Wikipedia
        let action_bitfield = mem::transmute::<_, &mut u64>(input_handler as usize + 0x10);
        let attacking = *action_bitfield | ATTACK != 0;
        let blocking = *action_bitfield | BLOCK != 0;
        let blocking_just_now = !BLOCKING_LAST_FRAME && blocking;


        // Keep track of the recent direction inputs
        let up = is_key_down(0x57);
        let down = is_key_down(0x53);
        let left = is_key_down(0x41);
        let right = is_key_down(0x44);
        let inputs = BUFFER.update(up, down, left, right);
        
        let desired_art = if blocking_just_now && BUFFER.aborted() {
            // when there're no recent inputs and the block button is just pressed, roll back to the default art
            // also manually clear the input buffer so the desired art in the next few frames will still be the default art
            BUFFER.clear(); 
            CONFIG.default_art
        } else {
            // Switch to the desired combat arts if the player is giving directional inputs
            CONFIG.arts.get(&inputs)
        };

        // equip the desired combat art or the fallback version
        if let Some(desired_art) = desired_art {
            let equipped = set_combat_art(desired_art);
            if !equipped {
                // look for possible fallback
                let fall_back = match desired_art {
                    ICHIMONJI_DOUBLE =>         Some(ICHIMONJI),
                    PRAYING_STRIKES_EXORCISM => Some(PRAYING_STRIKES),
                    HIGH_MONK =>                Some(SENPO_LEAPING_KICKS),
                    SHADOWFALL =>               Some(SHADOWRUSH),
                    EMPOWERED_MORTAL_DRAW =>    Some(MORTAL_DRAW), 
                    _ => None
                };
                if let Some(fall_back) = fall_back {
                    set_combat_art(fall_back);
                }
            }
        }

        // quirky inputs like [Up, Up] or [Down, Up] clearly means combat arts intead of moving (who moves like that?)
        // in such cases, using only ATTACK to perform combat arts should be allowed
        if attacking && inputs.meant_for_art(){
            *action_bitfield |= BLOCK
        }

        BLOCKING_LAST_FRAME = blocking;
    }
    let f = unsafe{ mem::transmute::<_, fn(*const c_void, usize)->usize>(PROCESS_INPUT) };
    f(input_handler, arg)
}


//----------------------------------------------------------------------------
//
//  Wrappers of functions from the original program
//
//----------------------------------------------------------------------------

fn set_combat_art(uid: u32) -> bool {
    // equipping the same combat art again can unequip the combat art
    static mut LAST_UID: u32 = 0;
    if unsafe { uid == LAST_UID } {
        return true;
    }
    // Validate if the player has already obtained the combat art
    // If so, there should be a corresponding item (with an item ID) representing that art
    // The mapping from UIDs to item IDs is not cached since it will change when player loads other save files.
    // Putting random items into the combat art slot can cause severe bugs like losing Kusabimaru permantly
    let Some(item_id) = get_item_id(uid) else {
        return false;
    };
    // cast the combat art id to some sort of "equip data" array.
    static mut EQUIP_DATA: [u32;17] = [0;17];
    let data_pointer = unsafe {
        EQUIP_DATA[14] = item_id as u32;
        EQUIP_DATA.as_ptr()
    };
    _set_skill_slot(1, data_pointer, true);
    unsafe {LAST_UID = uid}
    return true;
}


/// When players obtain skills(combat arts/prosthetic tools), skills become items in the inventory.
/// Thus a skill has 2 IDs: its original UID and its ID as an item in the inventory.
/// When putting things into item slots, the latter shall be used.
fn get_item_id(uid: u32) -> Option<u64> {
    // we are going on an adventure of pointers
    let game_data = unsafe{ *(0x143D5AAC0 as *const usize)};
    if game_data == 0 {
        error!("game_data is null");
        return None
    }

    let player_game_data = unsafe { *((game_data + 0x8) as *const usize) };
    if player_game_data == 0 {
        error!("game_data is null");
        return None
    }

    let inventory_data = unsafe { *((player_game_data + 0x5B0) as *const usize)};
    if inventory_data == 0 {
        error!("inventory_data is null");
        return None;
    }
    let inventory_data = inventory_data + 0x10;

    // finnally get the object
    let item_id = _get_item_id(inventory_data as *const c_void, &uid);
    if item_id == 0xFFFFFFFF {
        return None;
    }
    Some(item_id)
}


//----------------------------------------------------------------------------
//
//  Functions from the original program
//
//----------------------------------------------------------------------------

// TODO use some derive macro to clean up this mess
const GET_ITEM_ID: usize = 0x140C3D680;
const SET_SKILL_SLOT: usize = 0x140D592F0;
const PLAY_UI_SOUND: usize = 0x1408CE960;


// When a player obtains combat arts/prosthetic tools, they become items in the inventory.
// When equipping combat arts/prosthetic tools, the items' IDs shall be used instead of the orignal IDs.
fn _get_item_id(inventory: *const c_void, id: &u32) -> u64 {
    let f = unsafe{ mem::transmute::<_, fn(*const c_void, id: &u32)->u64>(GET_ITEM_ID) };
    f(inventory, id)
}

// equip_slot: 1 represents the combat art slot. 0, 2 and 4 represents the prosthetic slots
// equip_data: data_pointer[14] is for combat art ID. data_pointer[16] is for prosthetics ID
fn _set_skill_slot(equip_slot: isize, equip_data: *const u32, ignore_equip_lock: bool) {    
    let f = unsafe{ mem::transmute::<_, fn(isize, *const u32, bool)>(SET_SKILL_SLOT) };
    f(equip_slot, equip_data, ignore_equip_lock);
}


// for debugging
#[allow(dead_code)]
fn _play_ui_sound(arg1: isize, arg2: isize) {
    let f = unsafe{ mem::transmute::<_, fn(isize, isize)>(PLAY_UI_SOUND) };
    f(arg1, arg2);
}



