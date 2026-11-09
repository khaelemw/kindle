//! Hooks into game functions, done without modifying game code so Arxan (which restores
//! patched code) has nothing to undo: Kindle swaps entries in the game's vtables instead
//! and calls the original through the saved pointer.
//!
//! Damage: every hit goes through `SprjChrDamageModule` vtable `+0x40`
//! `(damage module, attacker ChrIns, damage data)` (the module is `[ChrIns + 0x1F90] + 0x98`;
//! its owner is at `+0x08`). 1.15.2 has two damage-module vtables (both slots jump into
//! Arxan-obfuscated code, so the code itself can't be hooked). Damage data (from
//! DS3RuntimeScripting, re-checked live): `+0x00` attack power before the target's defence
//! (f32; scaling it scales the hit, though not linearly), `+0x44` AtkParam id.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};

use crate::memory;

/// Events delivered from hooks rather than polling.
pub const EVENTS: &[&str] =
    &["damage", "animation", "effect", "item_used", "item_lot", "bonfire_rested", "banner", "boss_defeated", "shop_buy", "boss_bar", "travel"];

const DAMAGE_SLOT: usize = 0x40;
const MAX_VTABLES: usize = 8;

static VTABLES: [AtomicUsize; MAX_VTABLES] = [const { AtomicUsize::new(0) }; MAX_VTABLES];
static ORIGINALS: [AtomicUsize; MAX_VTABLES] = [const { AtomicUsize::new(0) }; MAX_VTABLES];

/// What the damage data looks like to mods; written back after handlers run.
pub struct Damage {
    pub target: usize,
    pub attacker: usize,
    pub amount: f32,
    pub attack_id: i32,
}

fn original_for(vtable: usize) -> usize {
    (0..MAX_VTABLES).find(|&i| VTABLES[i].load(Ordering::Acquire) == vtable).map(|i| ORIGINALS[i].load(Ordering::Acquire)).unwrap_or(0)
}

/// Writes a pointer-sized value into read-only data (vtable slots, handler globals).
pub unsafe fn write_ptr(slot: usize, value: usize) -> bool {
    unsafe { write_slot(slot, value) }
}

unsafe fn write_slot(slot: usize, value: usize) -> bool {
    let mut old = PAGE_PROTECTION_FLAGS(0);
    let p = slot as *const std::ffi::c_void;
    if unsafe { VirtualProtect(p, 8, PAGE_READWRITE, &mut old) }.is_err() {
        return false;
    }
    // One aligned 8-byte write: other threads may be calling through the slot.
    unsafe { std::ptr::write_volatile(slot as *mut usize, value) };
    let mut ignored = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(p, 8, old, &mut ignored) };
    true
}

/// Writes bytes into read-only data (string literals).
pub unsafe fn write_bytes(addr: usize, bytes: &[u8]) -> bool {
    let mut old = PAGE_PROTECTION_FLAGS(0);
    let p = addr as *const std::ffi::c_void;
    if unsafe { VirtualProtect(p, bytes.len(), PAGE_READWRITE, &mut old) }.is_err() {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        unsafe { std::ptr::write_volatile((addr + i) as *mut u8, *b) };
    }
    let mut ignored = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(p, bytes.len(), old, &mut ignored) };
    true
}

/// Points `vtable`'s damage slot at Kindle's hook (once per vtable).
fn patch_vtable(vtable: usize) {
    if original_for(vtable) != 0 {
        return;
    }
    let slot = vtable + DAMAGE_SLOT;
    let Some(original) = memory::read_u64(slot).map(|v| v as usize) else { return };
    let hook = damage_hook as *const () as usize;
    if original == hook || original == 0 {
        return;
    }
    let Some(i) = (0..MAX_VTABLES).find(|&i| VTABLES[i].load(Ordering::Acquire) == 0) else {
        crate::warn!("damage hook: more damage-module types than expected; some hits won't be seen");
        return;
    };
    // Record the original before the slot can call into the hook.
    ORIGINALS[i].store(original, Ordering::Release);
    VTABLES[i].store(vtable, Ordering::Release);
    if unsafe { write_slot(slot, hook) } {
        let base = memory::game_module().map(|(b, _)| b).unwrap_or(0);
        crate::info!("damage hook: installed on vtable DarkSoulsIII.exe+{:#x}", vtable.wrapping_sub(base));
    } else {
        VTABLES[i].store(0, Ordering::Release);
        crate::warn!("damage hook: could not write the vtable");
    }
}

/// Hooks the damage-module vtables of every loaded character (new types are picked up as
/// characters of new kinds load). Cheap after the first call.
pub fn ensure_damage_hooks() {
    let Ok(list) = crate::chars::all() else { return };
    for c in list {
        let Some(container) = memory::read_u64(c.addr + 0x1F90).filter(|&p| p != 0) else { continue };
        let Some(module) = memory::read_u64(container as usize + 0x98).filter(|&p| p != 0) else { continue };
        if let Some(vt) = memory::read_u64(module as usize).filter(|&p| p != 0) {
            patch_vtable(vt as usize);
        }
    }
}

unsafe extern "C" fn damage_hook(module: usize, attacker: usize, data: usize) -> usize {
    let vtable = memory::read_u64(module).unwrap_or(0) as usize;
    let original = original_for(vtable);
    if original == 0 {
        return 0; // unreachable: only patched vtables point here
    }
    let call_original = || {
        let f: unsafe extern "C" fn(usize, usize, usize) -> usize = unsafe { std::mem::transmute(original) };
        unsafe { f(module, attacker, data) }
    };
    let Some(target) = memory::read_u64(module + 8).map(|v| v as usize) else { return call_original() };
    let (Some(amount), Some(attack_id)) = (memory::read::<f32>(data), memory::read::<i32>(data + 0x44)) else {
        return call_original();
    };
    let mut hit = Damage { target, attacker, amount, attack_id };
    let cancel = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::dispatch_damage(&mut hit))).unwrap_or(false);
    if cancel {
        return 0;
    }
    if hit.amount != amount && hit.amount.is_finite() {
        unsafe { std::ptr::write_volatile(data as *mut f32, hit.amount.max(0.0)) };
    }
    call_original()
}

// ── animation events ───────────────────────────────────────────────────────
//
// Every animation event any character starts goes through the play function
// (`hkbCharacter, i32 event[6]`; see anim.rs). It's ordinary code (not Arxan-wrapped), so it
// gets a regular inline hook, installed once a mod subscribes. Mods only see events while
// subscribed (ANIMATION_WANTED), since there are many every second.

pub static ANIMATION_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ANIMATION_HOOKED: std::sync::Once = std::sync::Once::new();

/// What mods see of an animation event; `id` is written back (None cancels it).
pub struct Animation {
    pub hkb: usize,
    pub id: Option<i32>,
}

static ANIMATION_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "animation hook",
    name: "animation",
    pattern: crate::anim::PLAY_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(animation_hook),
    without: "mods don't see animation events",
};

pub fn ensure_animation_hook() {
    ANIMATION_HOOKED.call_once(|| {
        // (anim.rs found it first: the hook changes the bytes its pattern matches.)
        let Some(f) = crate::anim::play_function() else { return };
        if let Ok(h) = ANIMATION_SITE.hook_at(f) {
            std::mem::forget(h); // for the life of the process
            crate::info!("animation hook: installed");
        }
    });
}

fn animation_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (hkb, event) = unsafe { ((*reg).rcx as usize, (*reg).rdx as usize) };
    let call_original = || {
        let f: unsafe extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(original) };
        unsafe { f(hkb, event) }
    };
    if !ANIMATION_WANTED.load(Ordering::Relaxed) || event == 0 {
        return call_original();
    }
    let Some(id) = memory::read::<i32>(event) else { return call_original() };
    let mut anim = Animation { hkb, id: Some(id) };
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::dispatch_animation(&mut anim)));
    match (ok, anim.id) {
        (Ok(()), None) => 0,
        (Ok(()), Some(new)) => {
            if new != id {
                unsafe { std::ptr::write_volatile(event as *mut i32, new) };
            }
            call_original()
        }
        _ => call_original(),
    }
}

// ── SpEffects applied to any character ─────────────────────────────────────
//
// `+0x8858E0` (1.15.2, unique pattern) is the core "apply SpEffect" every path funnels into
// (the general wrapper `+0x885830` with ~40 callers, plus two direct callers):
// `(ChrIns* target, i32 id, params*, ChrIns* source, 6 more stack args) -> bool applied`.
// Cancelling returns false, as the function itself does when it declines. The original is
// called with all ten arguments (stack ones copied from the caller's frame).

const EFFECT_PATTERN: &str = "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48 89 7c 24 20 41 54 41 56 41 57 \
    48 83 ec 50 f6 81 0c 1a 00 00 40 44 0f b6 b4 24 a8 00 00 00 49 8b e9 4d 8b e0 44 8b fa 48 8b f1 \
    40 b7 01 0f 84 ?? ?? ?? ?? 45 84 f6 0f 85 ?? ?? ?? ?? 4d 85 c9 0f 84 ?? ?? ?? ?? 49 8b c9 e8 ?? ?? ?? ?? 84 c0 0f";

pub static EFFECT_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static EFFECT_HOOKED: std::sync::Once = std::sync::Once::new();

/// What mods see of an SpEffect about to be applied; `id` None cancels it.
pub struct Effect {
    pub target: usize,
    pub source: usize,
    pub id: Option<i32>,
}

static EFFECT_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "effect hook",
    name: "effect apply",
    pattern: EFFECT_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(effect_hook),
    without: "mods don't see effects applied",
};

pub fn ensure_effect_hook() {
    EFFECT_HOOKED.call_once(|| {
        if EFFECT_SITE.install().is_some() {
            crate::info!("effect hook: installed");
        }
    });
}

type ApplyFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

fn effect_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (target, id, params, source) = (r.rcx, r.rdx as u32 as i32, r.r8, r.r9);
    let stack = |i: u64| memory::read_u64(r.rsp as usize + 0x28 + 8 * i as usize).unwrap_or(0);
    let call = |id: i32| -> usize {
        let f: ApplyFn = unsafe { std::mem::transmute(original) };
        unsafe { f(target, id as u32 as u64, params, source, stack(0), stack(1), stack(2), stack(3), stack(4), stack(5)) as usize }
    };
    if !EFFECT_WANTED.load(Ordering::Relaxed) || target == 0 {
        return call(id);
    }
    let mut effect = Effect { target: target as usize, source: source as usize, id: Some(id) };
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::dispatch_effect(&mut effect)));
    match (ok, effect.id) {
        (Ok(()), None) => 0,
        (Ok(()), Some(new)) => call(new),
        _ => call(id),
    }
}

// ── goods used by a player ─────────────────────────────────────────────────
//
// `+0x8B96C0` (1.15.2, unique pattern; a PlayerIns virtual, so no direct callers) runs when a
// used goods item takes effect: `(PlayerIns*, goods param id) -> bool`. It consumes the item
// through `+0x5861D0(EquipGameData, inventory index, keep)`, where `keep` is the player's
// "no goods consume" flag (PlayerIns + 0x1EEA bit 3, from the Grand Archives CE table).
// Mods that keep the item get that flag set around the original call, so everything else the
// game does on use still happens; only the consumption is skipped.

const ITEM_USE_PATTERN: &str = "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8b 01 8b f2 48 8b d9 ff 90 b8 01 00 00 \
    48 8b f8 48 85 c0 75 10 48 8b 5c 24 38 48 8b 74 24 40 48 83 c4 20 5f c3";
const NO_CONSUME: (usize, u8) = (0x1EEA, 3);

pub static ITEM_USE_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ITEM_USE_HOOKED: std::sync::Once = std::sync::Once::new();

/// A goods item a player used; `keep` leaves it in the inventory.
pub struct ItemUse {
    pub player: usize,
    pub goods_id: i32,
    pub keep: bool,
}

static ITEM_USE_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "item use hook",
    name: "item use",
    pattern: ITEM_USE_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(item_use_hook),
    without: "mods don't see items used",
};

pub fn ensure_item_use_hook() {
    ITEM_USE_HOOKED.call_once(|| {
        if ITEM_USE_SITE.install().is_some() {
            crate::info!("item use hook: installed");
        }
    });
}

fn item_use_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (player, goods) = unsafe { ((*reg).rcx as usize, (*reg).rdx as u32 as i32) };
    let call = || {
        let f: unsafe extern "C" fn(usize, u64) -> usize = unsafe { std::mem::transmute(original) };
        unsafe { f(player, goods as u32 as u64) }
    };
    if !ITEM_USE_WANTED.load(Ordering::Relaxed) || player == 0 {
        return call();
    }
    let mut used = ItemUse { player, goods_id: goods, keep: false };
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::dispatch_item_use(&mut used)));
    let flag_at = player + NO_CONSUME.0;
    if ok.is_err() || !used.keep {
        return call();
    }
    let Some(before) = memory::read::<u8>(flag_at) else { return call() };
    unsafe { std::ptr::write_volatile(flag_at as *mut u8, before | (1 << NO_CONSUME.1)) };
    let result = call();
    // Restore only our bit, in case the game changed others meanwhile.
    if let Some(now) = memory::read::<u8>(flag_at) {
        let restored = (now & !(1 << NO_CONSUME.1)) | (before & (1 << NO_CONSUME.1));
        unsafe { std::ptr::write_volatile(flag_at as *mut u8, restored) };
    }
    result
}

// ── item lots ──────────────────────────────────────────────────────────────
//
// Every item lot the game rolls starts as a request built by `+0x7BC6F0(request*, i32 lot,
// f32 luck in xmm2, u8)` (1.15.2, unique pattern): six callers, each building one, rolling it
// and awarding what comes out. Seen in game: an area's map treasure is rolled all at once as it
// loads (`+0x7C13D0`), an enemy's drop as it dies (`+0x7C0D30`), event awards by `+0x7C10F0`. The hook
// runs before it and edits the lot id in place, so the game's own code carries on with the
// luck argument untouched. The game sets the rolled lot's own pickup flag; a cancelled roll
// becomes lot -1, which the game's tables use for "no lot".

const ITEM_LOT_PATTERN: &str = "33 c0 f3 0f 11 51 04 89 11 89 41 0c 48 89 41 10 48 8b c1 44 88 49 08 c3";
const NO_LOT: i32 = -1;

pub static ITEM_LOT_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ITEM_LOT_HOOKED: std::sync::Once = std::sync::Once::new();

/// An item lot about to be rolled; `lot` None cancels it.
pub struct ItemLot {
    pub lot: Option<i32>,
}

static ITEM_LOT_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "item lot hook",
    name: "item lot roll",
    pattern: ITEM_LOT_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::JmpBack(item_lot_hook),
    without: "mods don't see item lots rolled",
};

pub fn ensure_item_lot_hook() {
    ITEM_LOT_HOOKED.call_once(|| {
        if ITEM_LOT_SITE.install().is_some() {
            crate::info!("item lot hook: installed");
        }
    });
}

fn item_lot_hook(reg: *mut ilhook::x64::Registers) {
    if !ITEM_LOT_WANTED.load(Ordering::Relaxed) {
        return;
    }
    let r = unsafe { &mut *reg };
    let lot = r.rdx as u32 as i32;
    if lot <= 0 {
        return;
    }
    // Which of the six callers this is (pickup, drop, reward), for the log.
    let from = memory::read_u64(r.rsp as usize).unwrap_or(0) as usize;
    let base = memory::game_module().map(|(b, _)| b).unwrap_or(0);
    crate::debug!("item lot {lot} rolled (from DarkSoulsIII.exe+{:#x})", from.wrapping_sub(base));
    let mut roll = ItemLot { lot: Some(lot) };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::dispatch_item_lot(&mut roll))).is_err() {
        return;
    }
    let new = roll.lot.unwrap_or(NO_LOT);
    if new != lot {
        r.rdx = (r.rdx & !0xFFFF_FFFF) | new as u32 as u64;
    }
}

// -- talk scripts: trace and menu options -----------------------------------------
//
// NPC, bonfire and shop menus are talk scripts (ESD). Their commands go through
// `SprjEzStateTalkEvent` vtable slot 1 `(this, EzStateEvent*)` (a jump table over ids
// 0..0x72); their queries through `SprjEzStateTalkEnv` slot 1 `(this, value* out,
// EzStateQuery*)`. The event/query objects expose `+0x10` id, `+0x18` argument count and
// `+0x20` argument(i) -> {u64 value, u32 type}. The trace swaps those vtable slots and logs
// each call, to map ids to menu actions.

pub static TALK_TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TALK_EVENT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static TALK_ENV_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static TALK_HOOKED: std::sync::Once = std::sync::Once::new();

/// `id(args)` of a talk command (`query` false: slots +0x10 id, +0x18 count, +0x20
/// arg(i) -> value*) or query (`query` true: slots +0x08 id, +0x10 count, +0x18
/// arg(buf, i) -> value*). Values are {u64 raw, u32 type}.
fn describe_ezstate(obj: usize, query: bool) -> String {
    let vt = memory::read_u64(obj).unwrap_or(0) as usize;
    let slot = |off: usize| -> Option<usize> { memory::read_u64(vt + off).map(|f| f as usize) };
    let (id_off, count_off, arg_off) = if query { (0x08, 0x10, 0x18) } else { (0x10, 0x18, 0x20) };
    let (Some(get_id), Some(get_count), Some(get_arg)) = (slot(id_off), slot(count_off), slot(arg_off)) else {
        return "?".into();
    };
    let id: i32 = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(get_id)(obj) };
    let n: i32 = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(get_count)(obj) };
    let mut args = Vec::new();
    let mut buf = [0u64; 16];
    for i in 0..n.clamp(0, 16) {
        let v: usize = if query {
            unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize, *mut [u64; 16], i32) -> usize>(get_arg)(obj, &mut buf, i) }
        } else {
            unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize, i32) -> usize>(get_arg)(obj, i) }
        };
        if v == 0 {
            args.push("null".to_string());
            continue;
        }
        let raw = memory::read_u64(v).unwrap_or(0);
        let ty = memory::read::<u32>(v + 8).unwrap_or(0);
        args.push(format!("t{ty}:{raw:#x}"));
    }
    format!("{id}({})", args.join(", "))
}

/// id and integer arguments of a talk command (argument 0 is the id itself).
fn command_args(event: usize) -> (i32, Vec<i32>) {
    let vt = memory::read_u64(event).unwrap_or(0) as usize;
    let slot = |off: usize| memory::read_u64(vt + off).map(|f| f as usize);
    let (Some(get_id), Some(get_count), Some(get_arg)) = (slot(0x10), slot(0x18), slot(0x20)) else {
        return (-1, Vec::new());
    };
    let id: i32 = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(get_id)(event) };
    let n: i32 = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(get_count)(event) };
    let mut args = Vec::new();
    for i in 0..n.clamp(0, 8) {
        let v: usize = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize, i32) -> usize>(get_arg)(event, i) };
        let value = if v == 0 {
            0
        } else {
            match memory::read::<u32>(v + 8).unwrap_or(0) {
                1 => f32::from_bits(memory::read::<u32>(v).unwrap_or(0)) as i32,
                _ => memory::read::<i32>(v).unwrap_or(0),
            }
        };
        args.push(value);
    }
    (id, args)
}

/// id of a talk query.
fn query_id(query: usize) -> i32 {
    let vt = memory::read_u64(query).unwrap_or(0) as usize;
    match memory::read_u64(vt + 0x08) {
        Some(f) => unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(f as usize)(query) },
        None => -1,
    }
}

unsafe extern "C" fn talk_event_hook(this: usize, event: usize) -> usize {
    let dispatch = TALK_EVENT_ORIGINAL.load(Ordering::Acquire);
    let f: unsafe extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(dispatch) };
    let trace = TALK_TRACE.load(Ordering::Relaxed);
    let menus = crate::talk_menu::ACTIVE.load(Ordering::Relaxed);
    let travel = crate::bonfire::TRAVEL_REQUESTED.answered();
    if !trace && !menus && !travel {
        return unsafe { f(this, event) };
    }
    if trace {
        crate::info!("talk command {}", describe_ezstate(event, false));
    }
    let (id, args) = command_args(event);
    if travel {
        if crate::bonfire::travel_refused(id) {
            return 0;
        }
        crate::bonfire::talk_went_on(id);
    }
    if menus && !crate::talk_menu::before_command(this, dispatch, id, &args) {
        return 0;
    }
    let r = unsafe { f(this, event) };
    if menus {
        crate::talk_menu::after_command(this, id, &args);
    }
    r
}

/// First integer argument of a talk query (after its id), if any.
fn query_first_arg(query: usize) -> Option<i32> {
    let vt = memory::read_u64(query)? as usize;
    let count: i32 = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> i32>(memory::read_u64(vt + 0x10)? as usize)(query) };
    if count < 2 {
        return None;
    }
    let get_arg = memory::read_u64(vt + 0x18)? as usize;
    let mut buf = [0u64; 16];
    let v: usize = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize, *mut [u64; 16], i32) -> usize>(get_arg)(query, &mut buf, 1) };
    if v == 0 {
        return None;
    }
    memory::read::<i32>(v)
}

unsafe extern "C" fn talk_env_hook(this: usize, out: usize, query: usize) -> usize {
    let dispatch = TALK_ENV_ORIGINAL.load(Ordering::Acquire);
    let f: unsafe extern "C" fn(usize, usize, usize) -> usize = unsafe { std::mem::transmute(dispatch) };
    let r = unsafe { f(this, out, query) };
    if TALK_TRACE.load(Ordering::Relaxed) {
        let value = memory::read_u64(out).unwrap_or(0);
        let ty = memory::read::<u32>(out + 8).unwrap_or(0);
        crate::info!("talk query {} = t{ty}:{value:#x}", describe_ezstate(query, true));
    }
    if out != 0 && crate::bonfire::TRAVEL_REFUSED.load(Ordering::Relaxed) && query_id(query) == crate::bonfire::TALK_TRAVEL_STATE {
        unsafe { std::ptr::write_volatile(out as *mut i32, 2) };
    }
    if crate::multiplayer::IN_SESSION.switch() == 1 && out != 0 && query_id(query) == crate::multiplayer::TALK_IN_SESSION {
        unsafe { std::ptr::write_volatile(out as *mut u64, 0) };
    }
    if out != 0
        && crate::multiplayer::NPC_READY.switch() == 1
        && query_id(query) == crate::multiplayer::TALK_NPC_EFFECT
        && query_first_arg(query) == Some(crate::multiplayer::NPC_READY_EFFECT)
    {
        unsafe { std::ptr::write_volatile(out as *mut u64, 1) };
    }
    if crate::talk_menu::ACTIVE.load(Ordering::Relaxed)
        && out != 0
        && crate::talk_menu::involved()
        && memory::read::<u32>(out + 8) == Some(2)
        && let Some(value) = memory::read::<i32>(out)
    {
        let seen = match query_id(query) {
            crate::talk_menu::QUERY_PICKED => crate::talk_menu::on_picked(value),
            crate::talk_menu::QUERY_MENU_OPEN => match query_first_arg(query) {
                Some(menu_type) => crate::talk_menu::on_menu_open(this, dispatch, menu_type, value),
                None => value,
            },
            _ => value,
        };
        if seen != value {
            unsafe { std::ptr::write_volatile(out as *mut i32, seen) };
        }
    }
    r
}

/// Installs the talk dispatcher hooks (once).
pub fn ensure_talk_hooks() -> Result<(), String> {
    let mut result = Ok(());
    TALK_HOOKED.call_once(|| {
        let hook = |class: &str, original: &AtomicUsize, hook: usize| -> Result<(), String> {
            let vt = crate::patterns::find_vtable(class).ok_or_else(|| format!("{class} not found"))?;
            let slot = vt + 8;
            let orig = memory::read_u64(slot).ok_or("vtable unreadable")? as usize;
            original.store(orig, Ordering::Release);
            if unsafe { write_slot(slot, hook) } { Ok(()) } else { Err("could not write the vtable".into()) }
        };
        result = hook("SprjEzStateTalkEvent@NS_SPRJ", &TALK_EVENT_ORIGINAL, talk_event_hook as *const () as usize)
            .and_then(|_| hook("SprjEzStateTalkEnv@NS_SPRJ", &TALK_ENV_ORIGINAL, talk_env_hook as *const () as usize));
        match &result {
            Ok(()) => crate::info!("talk hooks: installed"),
            Err(e) => crate::warn!("talk hooks: {e}"),
        }
    });
    result?;
    if TALK_EVENT_ORIGINAL.load(Ordering::Acquire) == 0 || TALK_ENV_ORIGINAL.load(Ordering::Acquire) == 0 {
        return Err("talk hooks are unavailable".into());
    }
    Ok(())
}

/// Turns the talk trace on or off.
pub fn set_talk_trace(on: bool) -> Result<(), String> {
    ensure_talk_hooks()?;
    TALK_TRACE.store(on, Ordering::Relaxed);
    Ok(())
}
