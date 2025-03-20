use std::ffi::c_void;

//----------------------------------------------------------------------------
//
//  Addresses of objects and functions from the original program
//
//----------------------------------------------------------------------------

pub const PROCESS_INPUT: usize = 0x140B2C190;
pub const GAME_DATA: usize = 0x143D5AAC0;
pub const WORLD_DATA: usize = 0x143D7A1E0;
pub const GET_ITEM_ID: usize = 0x140C3D680;
pub const SET_SLOT: usize = 0x140D592F0;
pub const SET_EQUIPED_PROTHSETIC: usize = 0x140A26150;

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
                unsafe { std::mem::transmute::<_, extern fn($($arg: $arg_ty),*)$(-> $ret_ty)?>($address as *const ())($($arg),*) }
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
    fn set_equipped_prosthetic(unknown: *const c_void, zero: u32, prosthetic_index: u32);
}



//----------------------------------------------------------------------------
//
//  Helper functions
//
//----------------------------------------------------------------------------

#[allow(unused)]
#[inline(always)]
pub unsafe fn resolve_pointer_chain<R, const N: usize>(root: usize, offsets: [usize;N]) -> *mut R {
    unsafe {
        let mut p = root as *mut ();
        for offset in offsets {
            p = *(p as *mut *mut ());
            if p.is_null() {
                log::warn!("Runs into null pointers when resolving pointer chain.");
                return p as *mut R;
            }
            p = p.byte_add(offset);
        }
        p as *mut R
    }
}