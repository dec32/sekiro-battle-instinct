use std::{fmt::Display, io, num::NonZero, path::Path};
use windows::Win32::{Foundation::ERROR_SUCCESS, UI::Input::{KeyboardAndMouse::*, XboxController::{XInputGetState, XINPUT_STATE}}};
use crate::{config::Config, frame::Frames, game::{self}, input::InputBuffer};


//----------------------------------------------------------------------------
//
//  Basic constants
//
//----------------------------------------------------------------------------

// MOD behavior
const BLOCK_INJECTION_DURATION: u8 = 10;
const ATTACK_SUPRESSION_DURATION: u8 = 2;
const PROSTHETIC_SUPRESSION_DURATION: u8 = 2;
const PROSTHETIC_ROLLBACK_COUNTDOWN: Frames = Frames::standard(120);

// UIDs
const ASHINA_CROSS: UID = 5500;
const ONE_MIND: UID = 6100;
const SAKURA_DANCE: UID = 7700;
const ICHIMONJI: UID = 5300;
const ICHIMONJI_DOUBLE: UID = 7100;
const PRAYING_STRIKES: UID = 5900;
const PRAYING_STRIKES_EXORCISM: UID = 7500;
const SENPO_LEAPING_KICKS: UID = 5800;
const HIGH_MONK: UID = 7400;
const SHADOWRUSH: UID = 6000;
const SHADOWFALL: UID = 7600;
const MORTAL_DRAW: UID = 5700;
const EMPOWERED_MORTAL_DRAW: UID = 7300;

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
    config: Config,
    buffer: InputBuffer,
    cur_art: Option<UID>,
    blocking_last_frame: bool,
    attacking_last_frame: bool,
    using_tool_last_frame: bool,
    swapout_countdown: Countdown,
    rollback_countdown: Countdown,
    attack_delay: u8,
    prosthetic_delay: u8,
    injected_blocks: u8,
    disable_block: bool,
    prev_slot: Option<ProstheticSlot>,
    ejection: Option<(ItemID, ProstheticSlot)>,
    gamepad: Gamepad,
}

impl Mod {
    pub const fn new() -> Mod {
        Mod {
            config: Config::new(),
            buffer: InputBuffer::new(),
            cur_art: None,
            blocking_last_frame: false,
            attacking_last_frame: false,
            using_tool_last_frame: false,
            swapout_countdown: Countdown::zero(),
            rollback_countdown: Countdown::zero(),
            attack_delay: 0,
            prosthetic_delay: 0,
            injected_blocks: 0,
            disable_block: false,
            prev_slot: None,
            ejection: None,
            gamepad: Gamepad::new(),
        }
    }

    pub fn load_config(&mut self, path: &Path) -> io::Result<()>{
        self.config = Config::load(path)?;
        Ok(())
    }

    pub fn process_input(&mut self, input_handler: &mut game::InputHandler) {
        /***** keystates *****/
        let w_down = is_key_down(VK_W);
        let a_down = is_key_down(VK_A);
        let s_down = is_key_down(VK_S);
        let d_down = is_key_down(VK_D);
        // bind R3/R4 to x1/x2 in the future
        let x1_down = is_key_down(VK_XBUTTON1);
        let x2_down = is_key_down(VK_XBUTTON2);

        /***** update the motion inputs *****/ 
        let inputs = if let Some((x, y)) = self.gamepad.get_left_pos().filter(|pos|*pos != (0, 0)) {
            self.buffer.update_joystick(x, y)
        } else {
            let up = w_down;
            let right = d_down;
            let down = s_down;
            let left = a_down;
            self.buffer.update_keys(up, right, down, left)
        };

        /***** parse the action bitflags *****/
        let action = &mut input_handler.action;
        let attacking = *action & ATTACK != 0;
        let blocking = *action & BLOCK != 0;
        let using_tool = *action & USE_PROSTHETIC != 0;
        let jumping = *action & JUMP != 0;
        let dodging = *action & DODGE != 0;
        let attacked_just_now = !self.attacking_last_frame && attacking;
        let blocked_just_now = !self.blocking_last_frame && blocking;

        /***** query the desired prosthetic tool *****/
        // notice that `using_tool` is shadowed and it has a different semantics
        // than `attacking`, `blocking`, `jumping`, etc
        let using_tool = using_tool
            | (x1_down && !self.config.tools_on_x1.is_empty())
            | (x2_down && !self.config.tools_on_x2.is_empty());
        let used_tool_just_now = !self.using_tool_last_frame && using_tool;

        let desired_tools = if used_tool_just_now {
            // equip the alternative tools only right before using them
            // so that the prosthetic slot doesn't change on plain character movement
            self.rollback_countdown = Countdown::new(PROSTHETIC_ROLLBACK_COUNTDOWN);
            let mut tools: &[UID] = &[];
            if tools.is_empty() && x1_down {
                tools = self.config.tools_on_x1;
            } 
            if tools.is_empty() && x2_down {
                tools = self.config.tools_on_x2;
            }
            if tools.is_empty() && blocking {
                tools = self.config.tools_for_block;
            }
            if tools.is_empty() && !self.buffer.expired() {
                tools = self.config.tools.get_or_default(inputs);
            }
            tools
        } else if self.rollback_countdown.is_done() {
            // equip the default tool as soon as it's availble
            let tools = self.config.tools.get_or_default([]);
            if tools.is_empty() {
                // notice that it's possible that the player does not have any default tool configured
                // in this case we need to rollback to the previous slot instead of the default tool
                if let Some(prev_slot) = self.prev_slot.take() {
                    activate_prosthetic_slot(prev_slot);
                }
                // the equipping code already handles the revert of ejected tools properly when there're default 
                // tools configured. revert at rollback is only for when there's no default tool configured
                if let Some((ejected_tool, orignal_slot)) = self.ejection.take() {
                    equip_prosthetic(ejected_tool, orignal_slot);
                } 
            }
            tools
        } else {
            self.rollback_countdown.count_on(!using_tool);
            &[]
        };

        /***** equip the desired prosthetic tool *****/ 
        // revert the ejected tool as soon as we move away from its original slot
        // so that if any other tool needs to be ejected, it can be stored into `self.ejection`
        let active_slot = get_active_prosthetic_slot();
        if let Some((ejected_tool, original_slot)) = self.ejection {
            if active_slot != original_slot {
                equip_prosthetic(ejected_tool, original_slot);
                self.ejection = None;
            }
        }
        if !desired_tools.is_empty() {
            if let Some(target_slot) = desired_tools.iter().copied().filter_map(locate_prosthetic_tool).next() {
                // when multiple tools are bind to the same inputs, use the already equiped one first
                if target_slot != active_slot {
                    // remembers the active slot and rollback to it later if there're not default tools configured
                    self.prev_slot.get_or_insert(active_slot);
                    activate_prosthetic_slot(target_slot);
                }
            } else {
                // if none equipped, check if the ejected one is desired
                let mut equipped = false;
                if let Some((ejected_tool, original_slot)) = self.ejection {
                    for tool in desired_tools.iter().copied() {
                        if tool.get_item_id() == Some(ejected_tool) {
                            equip_prosthetic(ejected_tool, original_slot);
                            equipped = true;
                            self.ejection = None;
                            break;
                        }
                    }
                }
                // use the first one (that is owned by the player) in the list as the last resort
                if !equipped {
                    // replace the tool in the active slot and remembers the ejected one for later revert
                    // notice that only the first ever ejected tool is remembered because the latter ones are 
                    // placed into the slot by the MOD but not the player. there's not point in reverting them
                    //
                    // placing the desired tool into some dedicated slot and activating that slot later can cause bugs.
                    // that is because `equip_prosthetic` must happen AFTER `activate_prosthetic_slot` when they occur 
                    // within the same tick. activating the dedicated slot BEFORE placing any tool into it solves
                    // this problem of course but now it triggers a disgusting flickering in the slot UI instead
                    //
                    // bugs and disgust are both unacceptable. that's why the code chooses a more complex approach:
                    // placing tools into arbitrary active slots (thus `activate_prosthetic_slot` is no longer needed)
                    // and keep track of the arbitrary `self.prev_slot`s and the original slots of `self.ejection`
                    let active_tool = get_prosthetic_tool(active_slot);
                    for tool in desired_tools.iter().copied() {
                        if equip_prosthetic(tool, active_slot) {
                            if let Some(active_tool) = active_tool {
                                self.ejection.get_or_insert((active_tool, active_slot));
                            }
                            break;
                        }
                    }
                }
            }
            self.prosthetic_delay = PROSTHETIC_SUPRESSION_DURATION;
        }

        /***** query the desired combat art *****/
        let mut performed_block_free_art_just_now = false;
        let performed_art_just_now = blocking && attacked_just_now;
        let desired_art = if !self.swapout_countdown.is_done() {
            // fix buggy behavior of sakura dacne, ashina cross and one mind
            // One Mind has two windows for animation bugs to happen
            // one after pressing ATTACK (sheathing) and one after releasing ATTACK (drawing)
            // the current (ugly) solution is to apply the cooldown after pressing ATTACK,
            // but only start counting it down after ATTACK is released
            self.swapout_countdown.count_on(!attacking);
            None
        } else if attacked_just_now && !self.buffer.expired() {
            // rolling back is postponed to when BLOCK is pressed
            // only switch combat arts right before they are performed or else bugs can happen
            // for example, doing it while using Sakura Dance triggers the falling animation of High Monk
            // to cancel that unexpected animation, block/combat art need to take place
            // thus the moment of switching is delayed to when block/combat art happens
            self.config.arts.get(inputs).inspect(|_|{
                if inputs.meant_for_art() {
                    performed_block_free_art_just_now = true;
                }
            })
        } else if blocked_just_now {
            if self.buffer.expired() {
                // when there're no recent inputs and the block button is just pressed, roll back to the default art
                // also manually clear the input buffer so the desired art in the next few frames will still be the default art
                self.buffer.clear();
                self.config.arts.get([])
            } else {
                self.config.arts.get(inputs)
            }
        } else {
            None
        };

        // if combat art switching happens too quick after performing certain combat arts
        // animation of other unrelated combat arts can be triggered
        if performed_art_just_now || performed_block_free_art_just_now && self.swapout_countdown.is_done() {
            self.swapout_countdown = Countdown::new(self.cur_art.swapout_cooldown())
        }

        /***** equip the desired combat art (or its fallback version) *****/ 
        if let Some(desired_art) = desired_art {
            let mut desired_art = desired_art;
            loop {
                if self.cur_art == Some(desired_art) {
                    break;
                }
                if set_combat_art(desired_art) {
                    self.cur_art = Some(desired_art);
                    self.attack_delay = ATTACK_SUPRESSION_DURATION;
                    break;
                }
                desired_art = match desired_art {
                    ICHIMONJI_DOUBLE =>         ICHIMONJI,
                    PRAYING_STRIKES_EXORCISM => PRAYING_STRIKES,
                    HIGH_MONK =>                SENPO_LEAPING_KICKS,
                    SHADOWFALL =>               SHADOWRUSH,
                    EMPOWERED_MORTAL_DRAW =>    MORTAL_DRAW,
                    _ => { break; }
                }
            }
        }

        /***** action injection *****/
        // inputs like [Up, Up] or [Down, Up] clearly means combat art usage intead of moving
        // in such cases, players can perform combat arts without pressing BLOCK,
        // because the mod injects the BLOCK action for them
        if performed_block_free_art_just_now {
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

        /***** action supression *****/
        // when binding umbrella to BLOCK|USE_PROSTHETIC, cross slash gets a bit harder to perform 
        // because now players need to release BLOCK first to prevent combat arts from happening
        // the solution is, of course, supress BLOCK for the player 
        if used_tool_just_now {
            self.disable_block = true;
        }
        if blocked_just_now || performed_block_free_art_just_now || !using_tool {
            self.disable_block = false;
        }
        if self.disable_block {
            *action &= !BLOCK;
        }
        // prosthetic tools may have extra keybind
        if using_tool {
            *action |= USE_PROSTHETIC;
        }

        // if ATTACK|BLOCK happens way too quick after combat art switching
        // Wirdwind Slash will be performed instead of the just equipped combat art
        // supressing the few ATTACK frames that happens right after combat art switching solves the bug
        if self.attack_delay > 0 {
            *action &= !ATTACK;
            self.attack_delay -= 1;
        }
        // similar principle also goes for prosthetic tools
        if self.prosthetic_delay != 0 {
            *action &= !USE_PROSTHETIC;
            self.prosthetic_delay -= 1;
        }

        /***** for next frame to refer to *****/
        self.attacking_last_frame = attacking;
        self.blocking_last_frame = blocking;
        self.using_tool_last_frame = using_tool;
    }
}

trait CombatArt: Sized {
    fn is_sheathed(self) -> bool;
    fn swapout_cooldown(self) -> Frames;
}

impl CombatArt for UID {
    fn is_sheathed(self) -> bool {
        matches!(self, ASHINA_CROSS | ONE_MIND)
    }

    fn swapout_cooldown(self) -> Frames {
        let frames = match self {
            ASHINA_CROSS => 75,
            ONE_MIND     => 240,
            SAKURA_DANCE => 60,
            _ => 40,
        };
        Frames::standard(frames)
    }
}

impl CombatArt for Option<UID> {
    fn is_sheathed(self) -> bool {
        self.map(CombatArt::is_sheathed).unwrap_or(false)
    }

    fn swapout_cooldown(self) -> Frames {
        self.map(CombatArt::swapout_cooldown).unwrap_or(Frames::standard(0))
    }
}

struct Countdown {
    value: u16,
    running: bool,
}

impl Countdown {
    const fn zero() -> Countdown {
        Countdown { value: 0, running: false }
    }

    fn new(value: Frames) -> Countdown {
        Countdown { value: value.as_actual(), running: false }
    }

    fn count(&mut self) {
       self.value -= 1;
       self.running = true;
    }

    fn count_on(&mut self, cond: bool) {
        if cond || self.running {
            self.count();
        }
    }

    fn is_done(&self) -> bool {
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
        let mut xinput_state = XINPUT_STATE::default();
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

/// UIDs are consistent through different save files.
pub type UID = u32;

/// When players obtain skills(combat arts/prosthetic tools), skills become items in the inventory.
/// Thus a skill has 2 IDs: its original UID and its ID as an item in the inventory.
/// When putting things into item slots, the latter shall be used.
/// The mapping from UIDs to item IDs is not cached since it will change when player loads other save files.
/// Putting random items into the item slots can cause severe bugs like losing Kusabimaru permantly
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ItemID(NonZero<u32>);
impl ItemID {
    #[inline(always)]
    pub fn new(value: u32) -> Option<ItemID> {
        NonZero::<u32>::new(value).map(|inner|ItemID(inner))
    }

    #[inline(always)]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl Display for ItemID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0.get(), f)
    }
}



// Conversion between UID and ItemId
trait ID: Display + Clone + Copy {
    fn get_item_id(self) -> Option<ItemID>;
}

impl ID for ItemID {
    #[inline(always)]
    fn get_item_id(self) -> Option<ItemID> {
        Some(self)
    }
}

impl ID for UID {
    #[inline(always)]
    fn get_item_id(self) -> Option<ItemID> {
        let inventory = &inventory_data().inventory;
        let item_id = game::get_item_id(inventory, &self);
        ItemID::new(item_id).filter(|it|it.get() < 0xFFFF)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProstheticSlot {
    S0 = PROSTHETIC_SLOT_0,
    S1 = PROSTHETIC_SLOT_1,
    S2 = PROSTHETIC_SLOT_2,
}

impl ProstheticSlot {
    #[inline(always)]
    fn as_slot_index(self) -> usize {
        self as usize
    }
    #[inline(always)]
    fn as_prosthetic_index(self) -> u32 {
        self as u32 / 2
    }
}

fn set_combat_art(art: impl ID) -> bool {
    set_slot(art, COMBAT_ART_SLOT as usize)
}

fn equip_prosthetic(tool: impl ID, slot: ProstheticSlot) -> bool {
    set_slot(tool, slot.as_slot_index())
}

fn set_slot(item: impl ID, slot_index: usize) -> bool {
    let Some(item_id) = item.get_item_id() else {
        return false;
    };
    let equip_data = &game::EquipData::new(item_id.get());
    game::set_slot(slot_index, equip_data, true);
    true
}

fn get_prosthetic_tool(slot: ProstheticSlot) -> Option<ItemID> {
    let items = &player_data().equiped_items;
    let item_id = items[slot.as_slot_index()];
    if item_id != 256 {
        ItemID::new(item_id)
    } else {
        None
    }
}

fn get_active_prosthetic_slot() -> ProstheticSlot {
    let active_prosthetic = player_data().activte_prosthetic;
    match active_prosthetic {
        0 => ProstheticSlot::S0,
        1 => ProstheticSlot::S1,
        2 => ProstheticSlot::S2,
        illegal_slot => unreachable!("Illegal prosthetic slot: {illegal_slot}")
    }
}

fn locate_prosthetic_tool(tool: impl ID) -> Option<ProstheticSlot> {
    let items = &player_data().equiped_items;
    let Some(item_id) = tool.get_item_id() else {
        return None
    };
    for slot in [ProstheticSlot::S0, ProstheticSlot::S1, ProstheticSlot::S2] {
        if items[slot.as_slot_index()] == item_id.get() {
            return Some(slot);
        }
    }
    None
}

fn activate_prosthetic_slot(slot: ProstheticSlot) {
    use std::ffi::c_void;
    let unknown = unsafe {
        let character_base: *const c_void = game::resolve_pointer_chain(game::WORLD_DATA, [0x88, 0x1F10, 0x10, 0xF8, 0x10, 0x18, 0x00]);
        *(character_base.byte_add(0x10) as *const *const c_void)
    };
    game::set_equipped_prosthetic(unknown, 0, slot.as_prosthetic_index());
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