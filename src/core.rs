use std::{io, mem, num::NonZero, path::Path};
use frame::{Fps, FrameCount};
use input::{InputBuffer, InputsExt};
use config::Config;
use windows::Win32::{Foundation::ERROR_SUCCESS, UI::Input::{KeyboardAndMouse::*, XboxController::XInputGetState}};
use crate::{config, frame, game::{self}, input};


//----------------------------------------------------------------------------
//
//  Basic constants
//
//----------------------------------------------------------------------------

// MOD behavior
const BLOCK_INJECTION_DURATION: u8 = 10;
const ATTACK_SUPRESSION_DURATION: u8 = 2;
const PROSTHETIC_SUPRESSION_DURATION: u8 = 2;

// combat art UIDs
const ASHINA_CROSS: u32 = 5500;
const ONE_MIND: u32 = 6100;
const SAKURA_DANCE: u32 = 7700;
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
const JUMP: u64 = 0x10;
const DODGE: u64 = 0x2000;
const USE_PROSTHETIC: u64 = 0x40040002;

// slot index
const COMBAT_ART_SLOT: u8 = 1;
const PROSTHETIC_SLOT_0: u8 = 0;
const PROSTHETIC_SLOT_1: u8 = 2;
const PROSTHETIC_SLOT_2: u8 = 4;

//----------------------------------------------------------------------------
//
//  Actual content of the mod
//
//----------------------------------------------------------------------------

pub struct Mod {
    fps: Fps,
    config: Config,
    buffer: InputBuffer,
    cur_art: Option<u32>,
    blocking_last_frame: bool,
    attacking_last_frame: bool,
    using_tool_last_frame: bool, 
    equip_cooldown: Cooldown,
    attack_cooldown: u8,
    prosthetic_cooldown: u8,
    injected_blocks: u8,
    gamepad: Gamepad,
}

impl Mod {
    pub const fn new_const() -> Mod {
        Mod {
            fps: Fps::new_const(),
            config: Config::new_const(),
            buffer: InputBuffer::new_const(),
            cur_art: None,
            blocking_last_frame: false,
            attacking_last_frame: false,
            using_tool_last_frame: false,
            equip_cooldown: Cooldown::zero(),
            attack_cooldown: 0,
            prosthetic_cooldown: 0,
            injected_blocks: 0,
            gamepad: Gamepad::new_const(),
        }
    }

    pub fn load_config(&mut self, path: &Path) -> io::Result<()>{
        self.config = Config::load(path)?;
        Ok(())
    }

    pub fn process_input(&mut self, input_handler: *mut game::InputHandler) {
        // If you forget what a bitfield is please refer to Wikipedia
        let action = unsafe { &mut input_handler.as_mut().expect("input_handler is null").action };
        let attacking = *action & ATTACK != 0;
        let blocking = *action & BLOCK != 0;
        let using_tool = *action & USE_PROSTHETIC != 0;
        let jumping = *action & JUMP != 0;
        let dodging = *action & DODGE != 0;
        let attacked_just_now = !self.attacking_last_frame && attacking;
        let blocked_just_now = !self.blocking_last_frame && blocking;
        let used_tool_just_now = !self.using_tool_last_frame && using_tool;

        self.fps.tick();
        self.buffer.update_fps(self.fps.get());

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

        // prosthetic tools
        let desired_tool = if used_tool_just_now {
            if self.buffer.expired() {
                self.config.get_default_skill().tool
            } else {
                self.config.get_default_skill().tool
            }
        } else {
            None
        };

        if let Some(desired_tool) = desired_tool {
            let cur_slot = get_active_prosthetic_slot();
            let tool_slot = locate_prosthetic_tool(desired_tool);
            if tool_slot != Some(cur_slot) {
                equip_prosthetic(desired_tool, cur_slot);
                self.prosthetic_cooldown = PROSTHETIC_SUPRESSION_DURATION
            }
        }
        if self.prosthetic_cooldown != 0 {
            *action &= !USE_PROSTHETIC;
            self.prosthetic_cooldown -= 1;
        }
        

        // combat arts
        let desired_art = if !self.equip_cooldown.done() {
            // fix buggy behavior of sakura dacne, ashina cross and one mind
            if self.cur_art == Some(ONE_MIND) {
                // One Mind has two windows for animation bugs to happen
                // one after pressing ATTACK (sheathing) and one after releasing ATTACK (drawing)
                // the current (ugly) solution is to apply the cooldown after pressing ATTACK,
                // but only start counting it down after ATTACK is released
                if !attacking || self.equip_cooldown.is_running() {
                    self.equip_cooldown.decr();
                }
            } else {
                self.equip_cooldown.decr();
            }
            self.cur_art
        } else if attacking && self.cur_art.is_sheathed() {
            // keep using the same combat art when the player is still sheathing
            self.cur_art
        } else if blocked_just_now && self.buffer.expired() {
            // when there're no recent inputs and the block button is just pressed, roll back to the default art
            // also manually clear the input buffer so the desired art in the next few frames will still be the default art
            self.buffer.clear();
            self.config.get_default_skill().art
        } else {
            // Switch to the desired combat arts if the player is giving motion inputs
            self.config.get_skill(&inputs).art
        };

        // equip the desired combat art or the fallback version
        let mut performed_art_just_now = blocking && attacked_just_now;
        if let Some(desired_art) = desired_art {
            performed_art_just_now |= inputs.meant_for_art() && !self.buffer.expired() && attacked_just_now;
            if self.cur_art == Some(SAKURA_DANCE) {
                // switching combat arts while using Sakura Dance triggers the falling animation of High Monk
                // to cancel that unexpected animation, block/combat art need to take place
                // thus the moment of switching is delayed to when block/combat art happens
                if blocked_just_now || performed_art_just_now {
                    self.set_combat_art(desired_art);
                }
            } else {
                self.set_combat_art(desired_art);
            }
        }

        // inputs like [Up, Up] or [Down, Up] clearly means combat art usage intead of moving
        // in such cases, players can perform combat arts without pressing BLOCK,
        // because the mod injects the BLOCK action for them
        if performed_art_just_now {
            *action |= BLOCK;
            self.injected_blocks = 1;
        } else if self.injected_blocks >= 1 {
            if jumping || dodging {
                // DODGE and JUMP cancel the injection because they cancel the combat art itself
                self.injected_blocks = 0
            } else if self.cur_art.is_sheathed() {
                // hold BLOCK for sheathing attacks as long as ATTACK is held until:
                // 1. the player decides to hold BLOCK by themself (that usually means cancelling)
                // 2. the player released the attack
                if attacking && !blocking {
                    *action |= BLOCK;
                } else {
                    self.injected_blocks = 0;
                }
            } else if self.injected_blocks < BLOCK_INJECTION_DURATION {
                // inject just a few frames for other art
                *action |= BLOCK;
                self.injected_blocks += 1;
            }
        }


        // if ATTACK|BLOCK happens way too quick after combat art switching
        // Wirdwind Slash will be performed instead of the just equipped combat art
        // supressing the few ATTACK frames that happens right after combat art switching solves the bug
        if self.attack_cooldown > 0 {
            *action &= !ATTACK;
            self.attack_cooldown -= 1;
        }

        // if combat art switching happens too quick after performing certain combat arts
        // animation of other unrelated combat arts can be triggered
        if performed_art_just_now && self.equip_cooldown.done() {
            let cooldown = self.cur_art.equip_cooldown().adjust_to(self.fps.get());
            self.equip_cooldown = Cooldown::new(cooldown)
        }

        self.attacking_last_frame = attacking;
        self.blocking_last_frame = blocking;
        self.using_tool_last_frame = using_tool;
    }


    fn set_combat_art(&mut self, art: u32) {
        // equipping the same combat art again can unequip the combat art
        if self.cur_art == Some(art) {
            return;
        }
        if set_combat_art(art) {
            self.cur_art = Some(art);
            self.attack_cooldown = ATTACK_SUPRESSION_DURATION;
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
            self.set_combat_art(fallback)
        }
    }
}

trait CombatArt {
    fn is_sheathed(self) -> bool;
    fn equip_cooldown(self) -> u16;
}

impl CombatArt for u32 {
    fn is_sheathed(self) -> bool {
        matches!(self, ASHINA_CROSS | ONE_MIND)
    }

    fn equip_cooldown(self) -> u16 {
        match self {
            ASHINA_CROSS => 75,
            ONE_MIND => 240,
            SAKURA_DANCE => 60,
            _ => 0,
        }
    }
}

impl CombatArt for Option<u32> {
    fn is_sheathed(self) -> bool {
        self.map(CombatArt::is_sheathed).unwrap_or(false)
    }

    fn equip_cooldown(self) -> u16 {
        self.map(CombatArt::equip_cooldown).unwrap_or(0)
    }
}

struct Cooldown {
    value: u16,
    running: bool,
}

impl Cooldown {
    const fn zero() -> Cooldown {
        Cooldown::new(0)
    }

    const fn new(value: u16) -> Cooldown {
        Cooldown { value, running: false }
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn decr(&mut self) {
       self.value -= 1;
       self.running = true;
    }

    fn done(&self) -> bool {
        self.value == 0
    }
}


//----------------------------------------------------------------------------
//
//  Wrappers of Windows APIs
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
    const XUSER_MAX_COUNT: u32 = 3;
    const XINPUT_RETRY_INTERVAL: u16 = 300;
    const fn new_const() -> Gamepad {
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
        for idx in self.latest_idx..self.latest_idx + Self::XUSER_MAX_COUNT {
            let idx = idx % Self::XUSER_MAX_COUNT;
            let res = unsafe { XInputGetState(idx, &mut xinput_state) };
            if res == ERROR_SUCCESS.0 {
                self.connected = true;
                self.latest_idx = idx;
                return Some((xinput_state.Gamepad.sThumbLX, xinput_state.Gamepad.sThumbLY))
            }
        }
        // failed. start countdown
        self.connected = false;
        self.countdown = Self::XINPUT_RETRY_INTERVAL;
        return None;
    }
}


//----------------------------------------------------------------------------
//
//  Wrappers of functions from the original game
//
//----------------------------------------------------------------------------

type ItemId = NonZero<u32>;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProstheticSlot {
    S0 = PROSTHETIC_SLOT_0,
    S1 = PROSTHETIC_SLOT_1,
    S2 = PROSTHETIC_SLOT_2,
}

/// When players obtain skills(combat arts/prosthetic tools), skills become items in the inventory.
/// Thus a skill has 2 IDs: its original UID and its ID as an item in the inventory.
/// When putting things into item slots, the latter shall be used.
fn get_item_id(uid: u32) -> Option<ItemId> {
    let inventory = &inventory_data().inventory;
    let uid = &uid;
    let item_id = game::get_item_id(inventory, uid);
    let item_id = ItemId::try_from(item_id).ok()?;
    if item_id.get() > 0xFFFF {
        return None;
    }
    Some(item_id)
}


fn set_combat_art(uid: u32) -> bool {
    set_slot(uid, COMBAT_ART_SLOT as usize)
}

fn equip_prosthetic(uid: u32, slot: ProstheticSlot) -> bool {
    set_slot(uid, slot as usize)
}

fn set_slot(uid: u32, slot_index: usize) -> bool {
    // Validate if the player has already obtained the combat art
    // If so, there should be a corresponding item (with an item ID) representing that art
    // The mapping from UIDs to item IDs is not cached since it will change when player loads other save files.
    // Putting random items into the combat art slot can cause severe bugs like losing Kusabimaru permantly
    let Some(item_id) = get_item_id(uid) else {
        return false;
    };
    let equip_data = &game::EquipData::new(item_id.get());
    game::set_slot(slot_index, equip_data, true);
    return true;
}

fn get_active_prosthetic_slot() -> ProstheticSlot {
    let active_prosthetic = player_data().activte_prosthetic;
    let active_slot = match active_prosthetic {
        0 => ProstheticSlot::S0,
        1 => ProstheticSlot::S1,
        2 => ProstheticSlot::S2,
        illegal_slot => unreachable!("Illegal active prosthetic slot: {illegal_slot}")
    };
    active_slot
}

fn locate_prosthetic_tool(uid: u32) -> Option<ProstheticSlot> {
    let slots = player_data().equiped_items;
    log::trace!("slots: {slots:?}");
    let Some(item_id) = get_item_id(uid) else {
        return None
    };
    for slot in [ProstheticSlot::S0, ProstheticSlot::S1, ProstheticSlot::S2] {
        if slots[slot as usize] == item_id.get() {
            return Some(slot);
        }
    }
    None
}

fn game_data<'a>() -> &'a game::GameData {
    unsafe { game::game_data().as_ref().expect("game_data is null.") }
}

fn player_data<'a>() -> &'a game::PlayerData {
    unsafe { game_data().player_data.as_ref().expect("player_data is null.") }
}

fn inventory_data<'a>() -> &'a game::InventoryData {
    unsafe { player_data().inventory_data.as_ref().expect("inventory_data is null") }
}