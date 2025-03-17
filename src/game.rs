use std::ffi::c_void;
use std::{mem, ptr};

//----------------------------------------------------------------------------
//
//  Addresses of objects and functions from the original program
//
//----------------------------------------------------------------------------

pub const PROCESS_INPUT: usize = 0x140B2C190;
const GAME_DATA: usize = 0x143D5AAC0;
const GET_ITEM_ID: usize = 0x140C3D680;
const SET_SLOT: usize = 0x140D592F0;
const SET_EQUIPED_PROTHSETIC: usize = 0x140A26150;

//----------------------------------------------------------------------------
//
//  Structs from the original program
//
//----------------------------------------------------------------------------

pub fn game_data() -> *const GameData {
    unsafe { *(GAME_DATA as *const *const GameData) }
}

#[repr(C)]
pub struct GameData { 
    _0: [u8;8], pub player_data: *const PlayerData 
}

#[repr(C)]
pub struct PlayerData { 
    _0: [u8;0x29c], pub equiped_items: [u32; 5],
    _1: [u8;0x06c], pub activte_prosthetic: u8,
    _2: [u8;0x293], pub inventory_data: *const InventoryData 
} 

#[repr(C)]
pub struct InventoryData { 
    _0: [u8;16], pub inventory: c_void 
}

#[repr(C)]
pub struct InputHandler { 
    _0: [u8;16], pub action: u64 
}

#[repr(C)]
pub struct EquipData { 
    _0: [u8;56], pub item_id: u32,
}

impl EquipData {
    pub fn new(item_id: u32) -> EquipData {
        EquipData { _0: [0;56], item_id }
    }
}


//----------------------------------------------------------------------------
//
//  Functions from the original program
//
//----------------------------------------------------------------------------

macro_rules! forward {
    (
        $(
            @[$address:expr]
            fn $name:tt($($arg:tt: $arg_ty:ty),*) $(-> $ret_ty:ty)?
        );*;
    ) => {
        $(
            #[inline(always)]
            #[allow(unused)]
            pub fn $name($($arg: $arg_ty),*) $(-> $ret_ty)? {
                unsafe { mem::transmute::<_, extern fn($($arg: $arg_ty),*)$(-> $ret_ty)?>($address as *const ())($($arg),*) }
            }
        )*
    };
}

forward! {
    @[GET_ITEM_ID]
    fn get_item_id(inventory: *const c_void, uid: *const u32) -> u32;

    @[SET_SLOT]
    fn set_slot(equip_slot: usize, equip_data: *const EquipData, ignore_equip_lock: bool);
    
    @[SET_EQUIPED_PROTHSETIC]
    fn set_equipped_prosthetic(unknown: *const c_void, zero: usize, prosthetic_index: usize);
}



//----------------------------------------------------------------------------
//
//  Helper functions
//
//----------------------------------------------------------------------------

#[allow(unused)]
unsafe fn resolve_pointers<R, const N: usize>(root: usize, offsets: [usize;N]) -> *const R {
    unsafe {
        let mut p = *(root as *const *const ());
        for offset in offsets {
            log::trace!("resloving pointer {p:?}");
            if p == ptr::null() {
                return ptr::null()
            }
            p = *((p as usize + offset) as *const *const ());
        }
        p as *const R
    }
}


// problematic garbage
// fn switch_prothsetic_tool(slot: ProstheticSlot) {
//     let unknown = unsafe { resolve_pointers(0x143D7A1E0, [0x88, 0x1F10, 0x10, 0xF8, 0x10, 0x18, 0x00, 0x10]) };
//     game::set_equipped_prosthetic(unknown, 0, slot as usize / 2);
// }


// #[repr(C)]
// pub struct Gamepad0 {
//     _0: [u8; 0x24C],
//     axis0: f32, _1: u32,
//     axis1: f32, _2: u32,
//     axis2: f32, _3: u32,
//     axis3: f32, _4: u32
// }