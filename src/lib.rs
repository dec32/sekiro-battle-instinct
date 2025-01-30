mod log;
mod input;
mod config;

use std::{ffi::{c_void, OsStr, OsString}, fs, mem, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, ptr::NonNull, sync::{Mutex, OnceLock}, thread, time::Duration, u8};
use anyhow::{anyhow, Result};
use input::{InputBuffer, InputsExt};
use minhook::MinHook;
use config::Config;
use windows::{core::{s, GUID, HRESULT, PCWSTR}, Win32::{Foundation::{GetLastError, ERROR_SUCCESS, HINSTANCE}, System::{LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW}, SystemInformation::GetSystemDirectoryW, SystemServices::DLL_PROCESS_ATTACH}, UI::Input::{KeyboardAndMouse::*, XboxController::XInputGetState}}};
use ::log::{debug, error, trace, warn};


//----------------------------------------------------------------------------
//
//  Some basic constants
//
//----------------------------------------------------------------------------

// MOD behavior
const HOOK_DELAY: Duration = Duration::from_secs(10);
const XUSER_MAX_COUNT: u32 = 3;
const XINPUT_RETRY_INTERVAL: u16 = 300;
const BLOCK_INJECTION_DURATION: u8 = 10;
const ATTACK_SUPRESSION_DURATION: u8 = 2;

// addresses of objects and functions from the original program
const GAME_DATA: usize = 0x143D5AAC0;
const PROCESS_INPUT: usize = 0x140B2C190;
const GET_ITEM_ID: usize = 0x140C3D680;
const SET_SKILL_SLOT: usize = 0x140D592F0;

// combat art UIDs
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
        let mut buf: Vec<u16> = vec![0;128];
        let len = unsafe { GetModuleFileNameW(dll_module, buf.as_mut_slice()) } as usize;
        let dll_path = PathBuf::from(OsString::from_wide(&buf[..len]));
        let dir_path = dll_path.parent().unwrap();
        chainload(dir_path);
        modulate(dir_path);
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

fn chainload(path: &Path) {
    _chainload(path).inspect_err(|e|error!("Failed to chainload other dinput8.dll files. {e}")).ok();
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
        debug!("Chainloaded dll: {name:?}");
    }
    Ok(())
}


//----------------------------------------------------------------------------
//
//  Initialize the MOD
//
//----------------------------------------------------------------------------

static MOD: Mutex<Mod> = Mutex::new(Mod::new());
static PROCESS_INPUT_ORIG: OnceLock<fn(*const c_void, usize) -> usize> = OnceLock::new();

fn modulate(path: &Path) {
    let path = path.join("battle_instinct.cfg");
    thread::spawn(||{
        // hooking fails if it starts too soon (MH_ERROR_UNSUPPORTED_FUNCTION)
        thread::sleep(HOOK_DELAY);
        _modulate(path).inspect_err(|e|error!("Errored occured when modulating. {e}"))
    });
}

fn _modulate(path: PathBuf) -> Result<()> {
    MOD.lock().unwrap().load_config(&path)?;
    unsafe {
        let process_input_orig = MinHook::create_hook(
            PROCESS_INPUT as *mut c_void,
            process_input as *mut c_void).map_err(|e|anyhow!("{e:?}"))?;
        PROCESS_INPUT_ORIG.set(mem::transmute(process_input_orig)).unwrap();
        MinHook::enable_all_hooks().map_err(|e|anyhow!("{e:?}"))?;
    }
    Ok(())
}

fn process_input(input_handler: &c_void, arg: usize) -> usize {
    MOD.lock().unwrap().process_input(input_handler);
    let process_input_orig = PROCESS_INPUT_ORIG.get().cloned().unwrap();
    process_input_orig(input_handler, arg)
}


//----------------------------------------------------------------------------
//
//  Actual content of the mod
//
//----------------------------------------------------------------------------

struct Mod {
    config: Config,
    buffer: InputBuffer,
    cur_art: u32,
    blocking_last_frame: bool,
    attacking_last_frame: bool,
    injected_frames: u8,
    supressed_frames: u8,
    gamepad: Gamepad,
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
            gamepad: Gamepad::new(),
        }
    }

    fn load_config(&mut self, path: &Path) -> Result<()>{
        self.config = Config::load(path)?;
        Ok(())
    }

    fn process_input(&mut self, input_handler: *const c_void) {
        // If you forget what a bitfield is please refer to Wikipedia
        let action_bitfield = unsafe{ mem::transmute::<_, &mut u64>(input_handler as usize + 0x10) };
        let attacking = *action_bitfield & ATTACK != 0;
        let blocking = *action_bitfield & BLOCK != 0;
        let attacked_just_now = !self.attacking_last_frame && attacking;
        let blocked_just_now = !self.blocking_last_frame && blocking;
        if attacked_just_now {
            trace!("Attack");
        }

        // TODO inject backward action for Nightjar Reversal
        // (0, 0) is filtered out so I can test the keyboard while the controller is still plugged in
        let inputs = if let Some((x, y)) = self.gamepad.get_left_pos().filter(|pos|*pos != (0, 0)) {
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
        } else if blocked_just_now && self.buffer.expired() {
            // when there're no recent inputs and the block button is just pressed, roll back to the default art
            // also manually clear the input buffer so the desired art in the next few frames will still be the default art
            self.buffer.clear();
            self.config.default_art
        } else {
            // Switch to the desired combat arts if the player is giving motion inputs
            self.config.arts.get(&inputs)
        };

        // equip the desired combat art or the fallback version
        if let Some(desired_art) = desired_art {
            self.set_combat_art(desired_art);
        }

        // inputs like [Up, Up] or [Down, Up] clearly means combat art usage intead of moving
        // in such cases, players can perform combat arts without pressing BLOCK, because the mod injects the BLOCK action for them
        if attacked_just_now && inputs.meant_for_art() && desired_art.is_some() && !self.buffer.expired(){
            *action_bitfield |= BLOCK;
            self.injected_frames = 1;
        } else if self.injected_frames >= 1 {
            if self.cur_art == ASHINA_CROSS {
                // hold BLOCK for ashina cross as long as ATTACK is held until:
                // 1. the player decides to hold BLOCK by themself (that usually means they want to cancel Ashina Cross)
                // 2. the player released the attack
                if attacking && !blocking{
                    *action_bitfield |= BLOCK;
                } else {
                    self.injected_frames = 0;
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

        self.attacking_last_frame = attacking;
        self.blocking_last_frame = blocking;
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

fn is_key_down(keycode: VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(keycode.0.into()) as u16 & 0x8000 != 0 }
}

// todo: add support for ps5 controllers
struct Gamepad {
    connected: bool,
    countdown: u16,
    latest_idx: u32,
}

impl Gamepad {
    const fn new() -> Gamepad {
        Gamepad { connected: false, countdown: 0, latest_idx: 0 }
    }

    fn get_left_pos(&mut self) -> Option<(i16, i16)> {
        // checking a disconnected controller slot requires device enumeration,
        // which can be a performance hit
        if self.countdown > 0 {
            self.countdown -= 1;
            return None;
        }
        // checking controllers
        let mut xinput_state = unsafe { mem::zeroed() };
        for idx in self.latest_idx..self.latest_idx + XUSER_MAX_COUNT {
            let idx = idx % XUSER_MAX_COUNT;
            let res = unsafe { XInputGetState(idx, &mut xinput_state) };
            if res == ERROR_SUCCESS.0 {
                self.connected = true;
                self.latest_idx = idx;
                return Some((xinput_state.Gamepad.sThumbLX, xinput_state.Gamepad.sThumbLY))
            }
        }
        // failed. start countdown
        self.connected = false;
        self.countdown = XINPUT_RETRY_INTERVAL;
        return None;
    }
}


//----------------------------------------------------------------------------
//
//  Wrappers of functions from the original program
//
//----------------------------------------------------------------------------

fn game_data() -> Option<NonNull<GameData>> {
    unsafe { *(GAME_DATA as *const Option<NonNull<GameData>>) }
}

/// When players obtain skills(combat arts/prosthetic tools), skills become items in the inventory.
/// Thus a skill has 2 IDs: its original UID and its ID as an item in the inventory.
/// When putting things into item slots, the latter shall be used.
fn get_item_id(uid: u32) -> Option<u64> {
    let inventory = unsafe { &game_data()?.as_ref().player_data?.as_ref().inventory_data?.as_ref().inventory };
    let uid = &uid;
    let item_id = _get_item_id(inventory, uid);
    if item_id == 0xFFFFFFFF {
        return None;
    }
    Some(item_id)
}


fn set_combat_art(uid: u32) -> bool {
    // Validate if the player has already obtained the combat art
    // If so, there should be a corresponding item (with an item ID) representing that art
    // The mapping from UIDs to item IDs is not cached since it will change when player loads other save files.
    // Putting random items into the combat art slot can cause severe bugs like losing Kusabimaru permantly
    let Some(item_id) = get_item_id(uid) else {
        return false;
    };
    let equip_data = EquipData {
        padding: [0; 52],
        prosthetic_tool_item_id: 0,
        combat_art_item_id: item_id
    };
    let equip_data = &equip_data;
    _set_skill_slot(1, equip_data, true);
    return true;
}


//----------------------------------------------------------------------------
//
//  Structs (or maybe classes) from the original program
//
//----------------------------------------------------------------------------

#[repr(C)]
struct GameData { padding: [u8;8], player_data: Option<NonNull<PlayerData>> }
#[repr(C)]
struct PlayerData { padding: [u8;1456], inventory_data: Option<NonNull<InventoryData>> }
#[repr(C)]
struct InventoryData { padding: [u8;16], inventory: c_void }
#[repr(C)]
struct EquipData { padding: [u8;52], combat_art_item_id: u64, prosthetic_tool_item_id: u64 }

//----------------------------------------------------------------------------
//
//  Functions from the original program
//
//----------------------------------------------------------------------------

macro_rules! ext {
    (fn $name:tt($($arg:tt: $arg_ty:ty),*) $(-> $ret_ty:ty)?, $address:expr) => {
        #[inline(always)]
        fn $name($($arg: $arg_ty),*) $(-> $ret_ty)? {
            unsafe { mem::transmute::<_, fn($($arg: $arg_ty),*)$(-> $ret_ty)?>($address as *const ())($($arg),*) }
        }
    };
}

// When a player obtains combat arts/prosthetic tools, they become items in the inventory.
// When equipping combat arts/prosthetic tools, the items' IDs shall be used instead of the orignal IDs.
ext!(fn _get_item_id(inventory: *const c_void, uid: *const u32) -> u64, GET_ITEM_ID);

// equip_slot: 1 represents the combat art slot. 0, 2 and 4 represents the prosthetic slots
ext!(fn _set_skill_slot(equip_slot: isize, equip_data: *const EquipData, ignore_equip_lock: bool), SET_SKILL_SLOT);
