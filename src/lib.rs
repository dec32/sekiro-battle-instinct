mod log;
mod input;
mod config;

use std::{ffi::{c_void, OsStr, OsString}, fs, mem, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, thread, time::Duration, u8};
use anyhow::{anyhow, Result};
use input::{InputBuffer, InputsExt};
use minhook::MinHook;
use config::Config;
use windows::{core::{s, GUID, HRESULT, PCWSTR}, Win32::{Foundation::{GetLastError, ERROR_SUCCESS, HINSTANCE}, System::{LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW}, SystemInformation::GetSystemDirectoryW, SystemServices::DLL_PROCESS_ATTACH}, UI::Input::{KeyboardAndMouse::*, XboxController::XInputGetState}}};
use ::log::{debug, error, trace};


//----------------------------------------------------------------------------
//
//  Some basic constants
//
//----------------------------------------------------------------------------

// MOD behavior
const HOOK_DELAY: Duration = Duration::from_secs(10);
const XINPUT_RETRY_INTERVAL: u16 = 300;
const BLOCK_INJECTION_DURATION: u8 = 10;
const ATTACK_SUPRESSION_DURATION: u8 = 2;

// some function pointers from the original game
const PROCESS_INPUT: usize  = 0x140B2C190;
const GET_ITEM_ID: usize = 0x140C3D680;
const SET_SKILL_SLOT: usize = 0x140D592F0;
const PLAY_UI_SOUND: usize = 0x1408CE960;

// Combat art UIDs
const ASHINA_CROSS: u32 = 5500;
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

// action bitfields
const ATTACK: u64 = 0x1;
const BLOCK: u64 = 0x4;
#[allow(unused)]
const JUMP: u64 =0x10;
#[allow(unused)]
const SWITCH_PROSTHETIC: u64 = 0x400;
#[allow(unused)]
const DODGE: u64 = 0x2000;
#[allow(unused)]
const USE_PROSTHETIC: u64 = 0x40040002; // you sure this is correct?

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
            modulate(dir_path).inspect_err(|e|error!("Errored occured when modulating. {e}")).ok();
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
        debug!("Chainloaded dll: {name:?}");
    }
    Ok(())
}



//----------------------------------------------------------------------------
//
//  Actual content of the mod
//
//----------------------------------------------------------------------------

// TODO use some cheap try_lock mechanism to gurantee single thread access
static mut MOD: Mod = Mod::new();

fn modulate(path: &Path) -> Result<()> {
    // hooking fails (MH_ERROR_UNSUPPORTED_FUNCTION) if it starts too soon
    thread::sleep(HOOK_DELAY);
    unsafe {
        // Loading configs
        let path = path.join("battle_instinct.cfg");
        MOD.load_config(&path)?;
        // Hijack the input processing function
        let orig = MinHook::create_hook(PROCESS_INPUT as *mut c_void, process_input as *mut c_void)
            .map_err(|e|anyhow!("{e:?}"))?;
        let orig = mem::transmute::<_, fn(*const c_void, usize) -> usize>(orig);
        MOD.orig = Some(orig); 
        MinHook::enable_all_hooks().unwrap();
    }
    Ok(())
}

fn process_input(input_handler: *const c_void, arg: usize) -> usize {
    unsafe { MOD.process_input(input_handler, arg) }
}

struct Mod {
    config: Config,
    buffer: InputBuffer,
    cur_art: u32,
    blocking_last_frame: bool,
    attacking_last_frame: bool,
    injected_frames: u8,
    supressed_frames: u8,
    // the original process_input function
    orig: Option<fn(*const c_void, usize) -> usize>,
}

impl Mod {
    const fn new() -> Mod {
        Mod {
            config: Config::new(),
            buffer: InputBuffer::new(),
            blocking_last_frame: false,
            attacking_last_frame: false,
            injected_frames: 0,
            supressed_frames: u8::MAX,
            cur_art: 0,
            orig: None,
        }
    }

    fn load_config(&mut self, path: &Path) -> Result<()>{
        self.config = Config::load(path)?;
        Ok(())
    }

    fn process_input(&mut self, input_handler: *const c_void, arg: usize) -> usize {
        // If you forget what a bitfield is please refer to Wikipedia
        let action_bitfield = unsafe{ mem::transmute::<_, &mut u64>(input_handler as usize + 0x10) };
        let attacking = *action_bitfield & ATTACK != 0;
        let blocking = *action_bitfield & BLOCK != 0;
        let attacked_just_now = !self.attacking_last_frame && attacking;
        let blocked_just_now = !self.blocking_last_frame && blocking;
        
        // TODO inject backward action for Nightjar Reversal
        // (0, 0) is filtered out so I can test the keyboard while the controller is still plugged in
        let inputs = if let Some((x, y)) = get_joystick_pos().filter(|pos|*pos != (0, 0)) {
            self.buffer.update_joystick(x, y)
        } else {
            let up = is_key_down(VK_W);
            let right = is_key_down(VK_D);
            let down = is_key_down(VK_S);
            let left = is_key_down(VK_A);
            self.buffer.update_keys(up, right, down, left)
        };

        let desired_art = if self.cur_art == ASHINA_CROSS && attacking {
            // keep using Ashina Cross when the player is waiting to strike
            Some(ASHINA_CROSS)
        } else if blocked_just_now && self.buffer.aborted() {
            // when there're no recent inputs and the block button is just pressed, roll back to the default art
            // also manually clear the input buffer so the desired art in the next few frames will still be the default art
            self.buffer.clear(); 
            self.config.default_art
        } else {
            // Switch to the desired combat arts if the player is giving directional inputs
            self.config.arts.get(&inputs)
        };

        // equip the desired combat art or the fallback version
        if let Some(desired_art) = desired_art {
            self.set_combat_art(desired_art);
        }

        // quirky inputs like [Up, Up] or [Down, Up] clearly means combat art usage intead of quirky walking (who walks like that?)
        // in such cases, players can perform combat arts without pressing BLOCK, because the mod injects the BLOCK action for them
        if attacked_just_now && inputs.meant_for_art() && !self.buffer.aborted() {
            *action_bitfield |= BLOCK;
            self.injected_frames = 1;
        } else if self.injected_frames >= 1 { 
            if self.cur_art == ASHINA_CROSS && attacking {
                // hold BLOCK for ashina cross as long as ATTACK is also held
                // until the player decides to hold BLOCK by themself (that usually means they want to cancel Ashina Cross)
                if blocking {
                    self.injected_frames = 0;
                } else {
                    *action_bitfield |= BLOCK;
                }
            } else if self.injected_frames < BLOCK_INJECTION_DURATION {
                // inject just a few frames for other art
                *action_bitfield |= BLOCK;
                self.injected_frames += 1;
            }
        }

        // if ATTACK|BLOCK happens way too quick after combat art switching
        // Wirdwind Slash will be performed instead of the just equipped combat art
        // supressing the few ATTACK frames that happens right after combat art switching solves the bug
        if self.supressed_frames < ATTACK_SUPRESSION_DURATION {
            *action_bitfield &= !ATTACK;
            self.supressed_frames += 1;
        }

        if *action_bitfield != 0 {
            trace!("Action: {:016x}", action_bitfield)
        }

        self.attacking_last_frame = attacking;
        self.blocking_last_frame = blocking;
        self.orig.unwrap()(input_handler, arg)
    }


    fn set_combat_art(&mut self, art: u32) {
        // equipping the same combat art again can unequip the combat art
        if self.cur_art == art {
            return;
        }
        if set_combat_art(art) {
            self.cur_art = art;
            self.supressed_frames = 0;
            return;
        }

        let fallback = match art {
            ICHIMONJI_DOUBLE =>         Some(ICHIMONJI),
            PRAYING_STRIKES_EXORCISM => Some(PRAYING_STRIKES),
            HIGH_MONK =>                Some(SENPO_LEAPING_KICKS),
            SHADOWFALL =>               Some(SHADOWRUSH),
            EMPOWERED_MORTAL_DRAW =>    Some(MORTAL_DRAW), 
            _ => None
        };
        if let Some(fallback) = fallback {
            self.set_combat_art(fallback);
        }
    }
}



//----------------------------------------------------------------------------
//
//  Wrappers for Windows APIs
//
//----------------------------------------------------------------------------

#[allow(unreachable_code)]
fn get_joystick_pos() -> Option<(i16, i16)> {
    // checking a disconnected controller slot requires device enumeration, which can be a performance hit
    // TODO where to put this COUNTDOWN variable?
    static mut COUNTDOWN: u16 = 0;
    unsafe {
        if COUNTDOWN > 0 {
            COUNTDOWN -= 1;
            return None;
        }
        let mut xinput_state = mem::zeroed();
        let res = XInputGetState(0, &mut xinput_state);
        if res != ERROR_SUCCESS.0 {
            COUNTDOWN = XINPUT_RETRY_INTERVAL;
            None
        } else {
            Some((xinput_state.Gamepad.sThumbLX, xinput_state.Gamepad.sThumbLY))
        }
    }
}

fn is_key_down(keycode: VIRTUAL_KEY) -> bool {
    unsafe{ GetKeyState(keycode.0.into()) as u16 & 0x8000 != 0 }
}



//----------------------------------------------------------------------------
//
//  Wrappers of functions from the original program
//
//----------------------------------------------------------------------------

fn set_combat_art(uid: u32) -> bool {
    // Validate if the player has already obtained the combat art
    // If so, there should be a corresponding item (with an item ID) representing that art
    // The mapping from UIDs to item IDs is not cached since it will change when player loads other save files.
    // Putting random items into the combat art slot can cause severe bugs like losing Kusabimaru permantly
    let Some(item_id) = get_item_id(uid) else {
        return false;
    };
    // cast the combat art id to some sort of "equip data" array.
    let mut equip_data: [u32;17] = [0;17];
    let data_pointer = {
        equip_data[14] = item_id as u32;
        equip_data.as_ptr()
    };
    _set_skill_slot(1, data_pointer, true);
    trace!("Switched to combat art: {uid}");
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



