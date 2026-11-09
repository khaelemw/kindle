//! HavokScript (HKS) bridge: the character behaviour scripts (`c0000.hks` etc.).
//!
//! The scripts ask the game questions with `env(id, ...)` / `isenv(...)` and perform actions
//! with `act(id, ...)`. Those are thin C wrappers that call a handler through a global
//! function pointer and push its integer result; FromSoft registers them per script state in
//! one function (`register(L, "env", fn)`, then "isenv", then "act"; 1.15.2 `+0x10B78F0`);
//! the handlers are found through the wrappers (see WRAPPER_PATTERN). The
//! handlers are swapped for Kindle's (data pointers, no code patched): Kindle reads the call's
//! arguments straight from the HKS stack, lets mods observe or override them, and calls the
//! original.
//!
//! HKS stack (Havok Script, a Lua 5.1 fork): `L+0x50` base, `L+0x48` top, 16-byte values
//! `{u32 type, u32 pad, u64 value}`; types: 0 nil, 1 boolean, 3 number (f32), 4 string.
//! The script's context: `+0x105BE40(L)` (global hkbSelf -> `+0x10`), taken from the env
//! handler's first call (at `+0x3E`); its owner ChrIns is `[[context] + 0x20]` (what the env
//! handler passes to the real implementation). Strings are read with the game's
//! `lua_tolstring(L, index, NULL)`, found through Havok's `hkbGetVariable` (registered by the
//! same kind of function, its call at `+0x32`).
//!
//! Mods can also have a script run an act, answer an env question or run HKS source
//! (requests, below), and edited scripts can call back into mods with `kindle(name, ...)`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::{memory, patterns};

const REGISTER_PATTERN: &str = "40 53 48 83 ec 20 4c 8d 05 ?? ?? ?? ?? 48 8d 15 ?? ?? ?? ?? 48 8b d9 e8 ?? ?? ?? ?? \
    4c 8d 05 ?? ?? ?? ?? 48 8d 15 ?? ?? ?? ?? 48 8b cb e8 ?? ?? ?? ?? 4c 8d 05 ?? ?? ?? ?? 48 8d 15 ?? ?? ?? ??";
/// The env, isenv and act wrappers: `push rbx; sub rsp,20h; mov rax,[rip+handler]; mov rbx,rcx;
/// test rax,rax; je +2; call rax`, in a row 0x40 apart (env and isenv call one handler, act the
/// next; 1.15.2 `+0x10B7DF0`). They're plain code; the registration function that names them is
/// kept encrypted by the game's protection except while it runs, so a search for it can miss.
const WRAPPER_PATTERN: &str = "40 53 48 83 ec 20 48 8b 05 ?? ?? ?? ?? 48 8b d9 48 85 c0 74 02 ff d0";
const WRAPPER_STRIDE: usize = 0x40;

/// The handler globals: (env/isenv, act).
static GLOBALS: OnceLock<Option<(usize, usize)>> = OnceLock::new();
static ENV_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static ACT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static CHARACTER_FN: AtomicUsize = AtomicUsize::new(0);
static TOLSTRING: OnceLock<Option<usize>> = OnceLock::new();
/// The player's ChrIns, refreshed by the frame task (the trace only shows the player).
static PLAYER: AtomicUsize = AtomicUsize::new(0);

/// Log the player's env/act calls (discovery aid).
pub static TRACE: AtomicBool = AtomicBool::new(false);
/// env/act ids some mod overrides or watches.
static ENV_IDS: Mutex<Option<HashSet<HksId>>> = Mutex::new(None);
static ACT_IDS: Mutex<Option<HashSet<HksId>>> = Mutex::new(None);
static ANY_ENV: AtomicBool = AtomicBool::new(false);
/// The player's character doesn't act (a co-op player who is out): its script's action requests
/// (ACTION_REQUEST, ACTION_HELD, ACTION_HOLD_TIME, ANY_ACTION_REQUEST) answer none.
static PLAYER_ACTIONS_STOPPED: AtomicBool = AtomicBool::new(false);
const ACTION_REQUESTS: [i32; 4] = [1106, 1107, 1108, 1109];
static ANY_ACT: AtomicBool = AtomicBool::new(false);
static GAME_THREAD: AtomicUsize = AtomicUsize::new(0);
static LOGGED_THREAD: AtomicBool = AtomicBool::new(false);

pub use crate::mods::HksId;

/// A value from (or for) the HKS stack.
#[derive(Clone, Debug, PartialEq)]
pub enum HksValue {
    Nil,
    Bool(bool),
    Number(f32),
    Str(String),
    Other(u32),
}

/// Target of the 7-byte RIP-relative instruction at `at` (displacement at +3).
fn rip_target(at: usize) -> Option<usize> {
    patterns::rip_target(at + 3, at + 7)
}

fn globals() -> Option<(usize, usize)> {
    *GLOBALS.get_or_init(|| {
        let all = patterns::find_all(WRAPPER_PATTERN);
        let handler = |wrapper: usize| rip_target(wrapper + 6);
        let found: Vec<(usize, usize)> = all
            .iter()
            .filter_map(|&env| {
                let (isenv, act) = (env + WRAPPER_STRIDE, env + 2 * WRAPPER_STRIDE);
                if !all.contains(&isenv) || !all.contains(&act) {
                    return None;
                }
                let (e, i, a) = (handler(env)?, handler(isenv)?, handler(act)?);
                (e == i && a == e + 8).then_some((e, a))
            })
            .collect();
        let [g] = found[..] else {
            crate::warn!("hks: the env/act wrappers weren't found ({} matches, {} in a row)", all.len(), found.len());
            return None;
        };
        let base = memory::game_module().map(|(b, _)| b).unwrap_or(0);
        crate::info!("hks: env handler at DarkSoulsIII.exe+{:#x}, act at +{:#x}", g.0 - base, g.1 - base);
        Some(g)
    })
}

fn tolstring() -> Option<usize> {
    *TOLSTRING.get_or_init(|| {
        let reg = patterns::find_with_string("hks hkb registration", REGISTER_PATTERN, 0x0D, "hkbGetVariable")?;
        call_target(rip_target(reg + 0x06)? + 0x32)
    })
}

/// The arguments of the current HKS call (1-based stack slots), read directly.
fn args(l: usize) -> Vec<HksValue> {
    let (Some(top), Some(base)) = (memory::read_u64(l + 0x48), memory::read_u64(l + 0x50)) else { return Vec::new() };
    let n = (top.saturating_sub(base) / 16).min(16) as usize;
    (0..n).map(|i| slot_value(l, base as usize + i * 16, i as i32 + 1)).collect()
}

/// The value in a stack slot (`index` is its stack index, for strings).
fn slot_value(l: usize, slot: usize, index: i32) -> HksValue {
    match memory::read::<u32>(slot).unwrap_or(0) {
        0 => HksValue::Nil,
        1 => HksValue::Bool(memory::read::<u32>(slot + 8).unwrap_or(0) != 0),
        3 => HksValue::Number(memory::read::<f32>(slot + 8).unwrap_or(0.0)),
        4 => HksValue::Str(string_value(l, index).unwrap_or_default()),
        t => HksValue::Other(t),
    }
}

/// Longest string Kindle reads back from a script.
const MAX_STRING: usize = 1 << 20;

/// The string at stack index `index` (1-based), through the game's lua_tolstring.
fn string_value(l: usize, index: i32) -> Option<String> {
    let f = tolstring()?;
    let f: unsafe extern "C" fn(usize, i32, *mut usize) -> *const u8 = unsafe { std::mem::transmute(f) };
    let mut len = 0usize;
    let p = unsafe { f(l, index, &mut len) } as usize;
    if p == 0 {
        return None;
    }
    let bytes = memory::read_bytes(p, len.min(MAX_STRING))?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The ChrIns whose script this is.
fn script_owner(l: usize) -> Option<usize> {
    let ctx = script_character(l)?;
    let inner = memory::read_u64(ctx)? as usize;
    let owner = memory::read_u64(inner + 0x20)? as usize;
    (owner != 0).then_some(owner)
}

/// The Havok context object the script belongs to.
fn script_character(l: usize) -> Option<usize> {
    let mut f = CHARACTER_FN.load(Ordering::Relaxed);
    if f == 0 {
        // The game's env handler (Kindle's swap keeps it in ENV_ORIGINAL).
        let env = match ENV_ORIGINAL.load(Ordering::Acquire) {
            0 => memory::read_u64(globals()?.0)? as usize,
            e => e,
        };
        if env == 0 {
            return None;
        }
        f = call_target(env + 0x3E)?;
        CHARACTER_FN.store(f, Ordering::Relaxed);
    }
    let f: unsafe extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(f) };
    let c = unsafe { f(l) };
    (c != 0).then_some(c)
}

fn note_thread() {
    if !LOGGED_THREAD.swap(true, Ordering::Relaxed) {
        let me = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() } as usize;
        let game = GAME_THREAD.load(Ordering::Relaxed);
        crate::info!("hks: scripts run on thread {me} (Kindle's frame task: {game}{})", if me == game { ", same" } else { ", different" });
    }
}

/// Recorded by the frame task, to tell whether HKS runs on the same thread.
pub fn set_game_thread() {
    let id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() } as usize;
    GAME_THREAD.store(id, Ordering::Relaxed);
}

fn call_id(args: &[HksValue]) -> Option<HksId> {
    match args.first() {
        Some(HksValue::Number(n)) => Some(HksId::Num(*n as i32)),
        Some(HksValue::Str(s)) => Some(HksId::Name(s.clone())),
        _ => None,
    }
}

fn wanted(ids: &Mutex<Option<HashSet<HksId>>>, id: &HksId) -> bool {
    ids.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.contains(id))
}

/// What a mod handler sees and may change.
pub struct Call {
    pub act: bool,
    pub character: Option<usize>,
    pub id: HksId,
    pub args: Vec<HksValue>,
    pub result: i32,
    /// For act: false blocks the action.
    pub allow: bool,
}

unsafe extern "C" fn env_handler(l: usize) -> i32 {
    let orig: unsafe extern "C" fn(usize) -> i32 = unsafe { std::mem::transmute(ENV_ORIGINAL.load(Ordering::Acquire)) };
    run_requests(l);
    let trace = TRACE.load(Ordering::Relaxed);
    let stopped = PLAYER_ACTIONS_STOPPED.load(Ordering::Relaxed);
    if !trace && !stopped && !ANY_ENV.load(Ordering::Relaxed) {
        return unsafe { orig(l) };
    }
    note_thread();
    let a = args(l);
    let result = unsafe { orig(l) };
    let Some(id) = call_id(&a) else { return result };
    let owner = script_owner(l);
    if stopped && matches!(id, HksId::Num(n) if ACTION_REQUESTS.contains(&n)) && owner.is_some() && owner == Some(PLAYER.load(Ordering::Relaxed)) {
        return 0;
    }
    if trace && owner.is_some() && owner == Some(PLAYER.load(Ordering::Relaxed)) {
        crate::trace_hks(false, &id, &a, result);
    }
    if !wanted(&ENV_IDS, &id) {
        return result;
    }
    let mut call = Call { act: false, character: owner, id, args: a, result, allow: true };
    crate::dispatch_hks(&mut call);
    call.result
}

unsafe extern "C" fn act_handler(l: usize) -> i32 {
    let orig: unsafe extern "C" fn(usize) -> i32 = unsafe { std::mem::transmute(ACT_ORIGINAL.load(Ordering::Acquire)) };
    run_requests(l);
    let trace = TRACE.load(Ordering::Relaxed);
    if !trace && !ANY_ACT.load(Ordering::Relaxed) {
        return unsafe { orig(l) };
    }
    note_thread();
    let a = args(l);
    let Some(id) = call_id(&a) else { return unsafe { orig(l) } };
    let owner = script_owner(l);
    let traced = trace && owner.is_some() && owner == Some(PLAYER.load(Ordering::Relaxed));
    if wanted(&ACT_IDS, &id) {
        let mut call = Call { act: true, character: owner, id: id.clone(), args: a.clone(), result: 0, allow: true };
        crate::dispatch_hks(&mut call);
        if !call.allow {
            if traced {
                crate::trace_hks(true, &id, &a, -1);
            }
            return 0;
        }
    }
    let result = unsafe { orig(l) };
    if traced {
        crate::trace_hks(true, &id, &a, result);
    }
    result
}

/// Swaps the env/act handlers for Kindle's (once).
pub fn ensure_hooks() -> Result<(), String> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| {
        let (env_g, act_g) = globals().ok_or("the HKS env/act handlers weren't found on this game build")?;
        let swap = |global: usize, original: &AtomicUsize, ours: usize| -> Result<(), String> {
            let cur = memory::read_u64(global).ok_or("handler unreadable")? as usize;
            if cur == 0 {
                return Err("HKS handlers aren't installed yet".into());
            }
            original.store(cur, Ordering::Release);
            if unsafe { crate::hooks::write_ptr(global, ours) } { Ok(()) } else { Err("could not swap the handler".into()) }
        };
        swap(env_g, &ENV_ORIGINAL, env_handler as *const () as usize)?;
        swap(act_g, &ACT_ORIGINAL, act_handler as *const () as usize)?;
        crate::info!("hks: env/act hooks installed");
        Ok(())
    })
    .clone()
}

/// Sets which env/act ids mods watch (from the runtime, when handlers change).
pub fn set_watched(env: HashSet<HksId>, act: HashSet<HksId>) {
    ANY_ENV.store(!env.is_empty(), Ordering::Relaxed);
    ANY_ACT.store(!act.is_empty(), Ordering::Relaxed);
    *ENV_IDS.lock().unwrap_or_else(|e| e.into_inner()) = Some(env);
    *ACT_IDS.lock().unwrap_or_else(|e| e.into_inner()) = Some(act);
}

/// Stops the player's character acting, or lets it again (see PLAYER_ACTIONS_STOPPED).
pub fn set_player_actions_stopped(on: bool) {
    if on && let Err(e) = ensure_hooks() {
        crate::warn!("hks: the player's actions can't be stopped ({e})");
    }
    PLAYER_ACTIONS_STOPPED.store(on, Ordering::Relaxed);
}

/// Records the player's ChrIns (from the frame task).
pub fn set_player(chr: usize) {
    PLAYER.store(chr, Ordering::Relaxed);
}

// -- id names ---------------------------------------------------------------------
//
// Scripts call env/act with a number (most ids) or a Japanese name (a separate set: the exe
// maps those names to its own numbering). The game has no English names; Kindle's come from
// the player script: which of its functions makes each call (its function names survive, e.g.
// env 225 is asked by GetEquipType), the values seen while acting, and the script's own
// constants (HAND_*, WEIGHT_*, CONDITION_TYPE_*, ACTION_ARM_*). `verified` ones were confirmed
// in game: by changing their value or blocking them, or by matching game data (HP, FP, param
// fields of the equipped weapons). The rest are well-founded guesses, to be confirmed.

/// A known id: a number or the Japanese name scripts call it by.
pub enum KnownId {
    Num(i32),
    Name(&'static str),
}

use KnownId::{Name, Num};

/// (English name, id, verified, meaning)
pub const ENV_NAMES: &[(&str, KnownId, bool, &str)] = &[
    ("HP", Num(1000), true, "current HP"),
    ("STAMINA", Num(1001), true, "current stamina"),
    ("FP", Num(2016), true, "current FP"),
    ("HAS_SPEFFECT", Num(1116), true, "env(1116, id): 1 if the character has SpEffect id"),
    ("WEAPON_MOTION_CATEGORY", Num(225), true, "env(225, hand): the hand's weapon motion category (wepmotionCategory; 42 = fists; hand 0 left, 1 right)"),
    ("GUARD_MOTION_CATEGORY", Num(278), true, "env(278, hand): the hand's guard motion category (GUARD_STYLE_*: 0 default, 1 small, 2 large, 3 torch)"),
    ("WEAPON_ART_CATEGORY", Num(345), true, "env(345, hand): the hand's weapon art category (spAtkcategory)"),
    ("HAND_STYLE", Num(207), true, "how weapons are held (HAND_*: 1 right one-handed, 2 two-handing left, 3 two-handing right)"),
    ("EQUIP_WEIGHT", Num(257), false, "equip load class (WEIGHT_*: 0 light, 5 normal, 10 heavy, 15 overweight, 16 super light)"),
    (
        "ACTION_REQUEST",
        Num(1106),
        false,
        "env(1106, action): 1 when that action's input is requested (ACTION_ARM_*: 0 R1, 1 R2, 2 L1, 3 L2, 7 use item, 12 backstep, 13 roll, 14 jump)",
    ),
    ("ACTION_HELD", Num(1107), false, "env(1107, action): 1 while that action's input is held"),
    ("ACTION_HOLD_TIME", Num(1108), false, "env(1108, action): how long that action's input has been held"),
    ("ANY_ACTION_REQUEST", Num(1109), false, "1 when any action input is requested (IsActionRequest)"),
    (
        "HAS_CONDITION",
        Num(305),
        false,
        "env(305, type): 1 if the character is in that condition (CONDITION_TYPE_*: 117 stone, 136 crystal, 154 no estus, 268 hollowing death, 286/287 gravity)",
    ),
    ("ITEM_ANIM_TYPE", Num(231), false, "animation type of the item being used (ITEM_*: 10 drink, 19 drink FP, 21 drink sake)"),
    ("WEAPON_ART_INFO", Num(326), false, "env(326, hand): weapon art info for the hand (GetSwordArtsInfo)"),
    ("MOVE_CANCEL_TIMING", Num(2000), false, "1 when the current animation can be cancelled into movement"),
    ("GUARD_CANCEL_TIMING", Num(243), false, "1 when the current animation can be cancelled into guarding"),
    ("FALL_START", Num(200), false, "asked when a fall may start (ExecFallStart)"),
    ("FALLING_FOR", Num(315), true, "env(315, seconds): 1 once the character has been falling that long (the player's script kills it at 12)"),
    ("FRAME_TIME", Num(333), false, "frame time (GetDeltaTime; observed 18-25)"),
    ("LADDER_COMMAND", Num(105), false, "env(105, n): ladder event command (-1 none)"),
    ("IS_FEMALE", Name("女性か"), false, "1 if the character is female"),
    ("IS_DUAL_BLADE", Name("双剣か"), false, "env(name, hand): 1 if the hand holds a paired weapon"),
    ("WEAPON_ART_CATEGORY_BY_NAME", Name("装備武器特殊カテゴリ番号取得"), true, "env(name, hand): the hand's weapon art category (spAtkcategory)"),
    ("WEAPON_ART_CANCEL_TYPE", Name("剣戟キャンセルタイプを取得"), false, "env(name, hand): weapon art cancel type"),
    ("CAN_CANCEL_WEAPON_ART", Name("剣戟キャンセル可能か"), false, "1 when the weapon art can be cancelled"),
    ("IS_EQUIP_MENU_OPEN", Name("装備メニュー開いているか"), false, "1 while the equipment menu is open"),
    ("HAS_EVENT_ANIM_REQUEST", Name("イベントアニメ再生要求があるか"), false, "1 when an event animation is requested"),
    ("IDLE_ANIM_CATEGORY", Name("待機アニメカテゴリ取得"), false, "idle animation category"),
    ("IS_ANIM_END", Name("アニメ終了か"), false, "env(name, layer): 1 when the animation has ended"),
    ("WEAPON_STORAGE_TYPE", Name("武器格納場所タイプ取得"), false, "env(name, hand): where the weapon is stowed (back, waist, shoulder)"),
    ("UPPER_ARM_OUTER_ANGLE", Name("上腕制御外側角度"), false, "upper arm control: outward angle"),
    ("UPPER_ARM_VERTICAL_ANGLE", Name("上腕制御上下角度"), false, "upper arm control: vertical angle"),
];

/// (English name, id, verified, meaning)
pub const ACT_NAMES: &[(&str, KnownId, bool, &str)] = &[
    ("SET_VARIABLE", Num(148), true, "act(148, name, value): set a behaviour graph variable"),
    ("APPLY_SPEFFECT", Num(2002), true, "act(2002, id): apply SpEffect id"),
    ("ADD_STAMINA", Num(1001), false, "act(1001, amount): add (negative: spend) stamina"),
    ("SET_HAND_STYLE", Num(104), false, "act(104, style): change how weapons are held (HAND_*)"),
    ("SET_THROW_ATTACK_INVALID", Num(102), false, "act(102, n): disable throw attacks (SetThrowAtkInvalid)"),
    ("SET_WEAPON_ART_CANCEL_TYPE", Num(2011), false, "act(2011, type): weapon art cancel type"),
    ("SET_AI_ACTION_STATE", Num(9103), false, "act(9103, state): AI action state"),
    ("RESET_REQUEST", Num(9101), false, "clear pending action requests (ResetRequest)"),
    ("LOCKON_TURN_LIMIT", Name("ロックオン時システム旋回不可角度"), false, "act(name, a, b): angles the system won't turn to while locked on"),
    ("LOCKON_RELEASE_ANGLE_LOCK", Name("ロックオン中角度固定解除"), false, "release the angle lock while locked on"),
    ("AUTO_TARGET_SET", Name("自動捕捉対象設定"), false, "set the auto-aim target"),
    ("AUTO_TARGET_CLEAR", Name("自動捕捉対象クリア"), false, "clear the auto-aim target"),
    ("AI_NOTIFY_ATTACK_TYPE", Name("AI通知攻撃タイプ"), false, "act(name, type): attack type told to AI"),
    ("SET_4WAY_MOVE_THRESHOLD", Name("4方向移動閾値設定"), false, "act(name, a, b): four-way movement thresholds"),
    ("DISABLE_AVARICE_DRAIN", Name("貪欲者の印スリップダメージ無効"), false, "pause the Symbol of Avarice HP drain (done at bonfires)"),
    ("SET_IN_ITEM_ANIM", Name("アイテムアニメ中か設定"), false, "mark the character as in an item animation"),
    ("DISABLE_ITEM_SWITCH", Name("道具ID切り替え無効"), false, "disable switching the selected item"),
    ("SET_WEAPON_PARAM_HAND", Name("武器パラメータ参照先"), false, "act(name, hand): which hand's weapon params attacks use"),
];

impl KnownId {
    fn matches(&self, id: &HksId) -> bool {
        match (self, id) {
            (Num(a), HksId::Num(b)) => a == b,
            (Name(a), HksId::Name(b)) => a == b,
            _ => false,
        }
    }
}

/// Kindle's English name for an env (`act` false) or act id, if it knows it.
pub fn id_name(act: bool, id: &HksId) -> Option<&'static str> {
    let table = if act { ACT_NAMES } else { ENV_NAMES };
    table.iter().find(|(_, k, _, _)| k.matches(id)).map(|(n, _, _, _)| *n)
}

// -- Kindle -> script: requests ------------------------------------------------------
//
// A mod can have a character's script perform act(id, ...), ask env(id, ...) or run a chunk
// of HKS source. The request waits for that character's next env/act call (so it runs on the
// script's own thread, with its state valid). For env/act, Kindle pushes the arguments onto
// the script's stack with the game's push functions, points the stack base at them, calls the
// original handler (exactly the frame a normal call gives it) and restores base and top.
// Source is compiled with the state's luaL_loadbuffer and run with lua_pcall (both found
// through HKS's own `loadstring` and `pcall`). Results go back to the mod on the next frame.
// Kindle-made calls don't reach mods' on_env/on_act handlers.

const PUSH_NUMBER_PATTERN: &str = "48 8b 41 48 48 83 c0 10 c7 40 f0 03 00 00 00 f3 0f 11 48 f8 48 89 41 48 c3";
const PUSH_BOOLEAN_PATTERN: &str = "4c 8b 41 48 33 c0 85 d2 0f 95 c0 41 c7 00 01 00 00 00 41 89 40 08 49 8d 40 10 48 89 41 48 c3";
const PUSH_STRING_PATTERN: &str =
    "48 85 d2 75 10 48 8b 41 48 48 83 c0 10 89 50 f0 48 89 41 48 c3 49 83 c8 ff 0f 1f 80 00 00 00 00 49 ff c0 42 80 3c 02 00 75 f6 e9";
/// HKS `loadstring`; its luaL_loadbuffer call is at +0x48.
const LOADSTRING_PATTERN: &str = "48 89 5c 24 08 57 48 83 ec 20 4c 8d 44 24 38 ba 01 00 00 00 48 8b f9 48 c7 44 24 38 00 00 00 00 \
    e8 ?? ?? ?? ?? 45 33 c9 41 8d 51 02 4c 8b c0 48 8b cf 48 8b d8 e8 ?? ?? ?? ?? 4c 8b 44 24 38 48 8b d3 48 8b cf 4c 8b c8 e8";
/// HKS `pcall`; its lua_pcall call is at +0x30.
const PCALL_PATTERN: &str = "48 89 5c 24 08 57 48 83 ec 20 48 8b 59 48 ba 01 00 00 00 48 8b f9 48 2b 59 50 48 c1 fb 04 \
    e8 ?? ?? ?? ?? 8d 53 ff 45 33 c9 41 83 c8 ff 48 8b cf e8";
const CHUNK_NAME: &[u8] = b"=kindle\0";
/// Run before every eval chunk. The game's script states carry Lua's full io, os, package and
/// debug libraries (the game's own scripts use none of them); without them, a mod's chunk
/// can't reach files, programs or native libraries, keeping Kindle's sandbox intact. And the
/// state's `loadstring` and `load` refuse precompiled chunks (bytecode, which starts with byte 27):
/// crafted bytecode reads and writes memory in Lua 5.1-style engines, a way out of the sandbox.
/// They're wrapped once; the originals stay only inside the wrappers (no debug library to reach
/// them).
const SANITIZE: &str = "local loaded = rawget(_G, 'package') and package.loaded \
    for _, name in ipairs({'io', 'os', 'package', 'debug', 'require', 'module', 'dofile', 'loadfile', 'loadlib'}) do \
      rawset(_G, name, nil) \
      if loaded then loaded[name] = nil end \
    end \
    if rawget(_G, '__kindle_text_only') == nil then \
      local loadstring_, load_, byte = rawget(_G, 'loadstring'), rawget(_G, 'load'), string and string.byte \
      local function text(s) return type(s) == 'string' and byte ~= nil and byte(s, 1) ~= 27 end \
      if loadstring_ then rawset(_G, 'loadstring', function(s, ...) if not text(s) then return nil, 'precompiled chunks are refused' end return loadstring_(s, ...) end) end \
      if load_ then rawset(_G, 'load', function(s, ...) if not text(s) then return nil, 'precompiled chunks are refused' end return load_(s, ...) end) end \
      rawset(_G, '__kindle_text_only', true) \
    end \
    return rawget(_G, 'io') == nil and rawget(_G, 'os') == nil and rawget(_G, 'debug') == nil and rawget(_G, '__kindle_text_only') == true";

/// Whether `source` is a precompiled chunk (bytecode), which hks.eval refuses (see SANITIZE).
pub(crate) fn precompiled(source: &str) -> bool {
    source.as_bytes().first() == Some(&0x1B)
}

static PUSH: OnceLock<Option<(usize, usize, usize)>> = OnceLock::new();
static EVAL: OnceLock<Option<(usize, usize)>> = OnceLock::new();

fn call_target(at: usize) -> Option<usize> {
    (memory::read::<u8>(at)? == 0xE8).then_some(())?;
    Some((at + 5).wrapping_add_signed(memory::read::<i32>(at + 1)? as isize))
}

fn push_fns() -> Option<(usize, usize, usize)> {
    *PUSH.get_or_init(|| {
        Some((
            patterns::find_unique("hks pushnumber", PUSH_NUMBER_PATTERN)?,
            patterns::find_unique("hks pushboolean", PUSH_BOOLEAN_PATTERN)?,
            patterns::find_unique("hks pushstring", PUSH_STRING_PATTERN)?,
        ))
    })
}

/// (luaL_loadbuffer, lua_pcall)
fn eval_fns() -> Option<(usize, usize)> {
    *EVAL.get_or_init(|| {
        let loadstring = patterns::find_unique("hks loadstring", LOADSTRING_PATTERN)?;
        let pcall = patterns::find_unique("hks pcall", PCALL_PATTERN)?;
        Some((call_target(loadstring + 0x48)?, call_target(pcall + 0x30)?))
    })
}

pub enum RequestKind {
    Env,
    Act,
    /// HKS source to run in the character's script state.
    Eval(String),
}

pub struct Request {
    pub id: u64,
    pub owner: usize,
    pub kind: RequestKind,
    /// env/act: the id first, then its arguments.
    pub args: Vec<HksValue>,
    pub frame: u64,
}

/// How a request ended.
pub enum Reply {
    /// env/act's result.
    Int(i32),
    /// The first value an eval chunk returned.
    Value(HksValue),
    /// The chunk didn't compile, or raised an error.
    Error(String),
    /// The character's script didn't run in time.
    Expired,
}

static REQUESTS: Mutex<Vec<Request>> = Mutex::new(Vec::new());
static HAS_REQUESTS: AtomicBool = AtomicBool::new(false);
static RESULTS: Mutex<Vec<(u64, Reply)>> = Mutex::new(Vec::new());
/// Frames a request waits for its character's script.
const REQUEST_FRAMES: u64 = 300;

/// Queues a request (installs the hooks if needed).
pub fn request(r: Request) -> Result<(), String> {
    ensure_hooks()?;
    push_fns().ok_or("the HKS push functions weren't found on this game build")?;
    match &r.kind {
        RequestKind::Eval(_) => {
            eval_fns().ok_or("the HKS compiler wasn't found on this game build")?;
        }
        _ if r.args.len() > 8 => return Err("too many arguments (at most 8)".into()),
        _ => {}
    }
    REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).push(r);
    HAS_REQUESTS.store(true, Ordering::Relaxed);
    Ok(())
}

/// Results of finished (or expired) requests since the last call.
pub fn take_results(frame: u64) -> Vec<(u64, Reply)> {
    let mut q = REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::mem::take(&mut *RESULTS.lock().unwrap_or_else(|e| e.into_inner()));
    q.retain(|r| {
        let keep = frame.saturating_sub(r.frame) <= REQUEST_FRAMES;
        if !keep {
            out.push((r.id, Reply::Expired));
        }
        keep
    });
    HAS_REQUESTS.store(!q.is_empty(), Ordering::Relaxed);
    out
}

/// Pushes a value onto the script's stack.
unsafe fn push(l: usize, v: &HksValue) -> Option<()> {
    let (num, boolean, string) = push_fns()?;
    unsafe {
        match v {
            HksValue::Number(n) => {
                let f: unsafe extern "C" fn(usize, f32) = std::mem::transmute(num);
                f(l, *n);
            }
            HksValue::Bool(b) => {
                let f: unsafe extern "C" fn(usize, i32) = std::mem::transmute(boolean);
                f(l, *b as i32);
            }
            HksValue::Str(s) => {
                // The game copies the string.
                let cs = std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default();
                let f: unsafe extern "C" fn(usize, *const std::ffi::c_char) = std::mem::transmute(string);
                f(l, cs.as_ptr());
            }
            _ => {
                let top = memory::read_u64(l + 0x48)?;
                std::ptr::write_volatile(top as *mut u32, 0);
                std::ptr::write_volatile((l + 0x48) as *mut u64, top + 16);
            }
        }
    }
    Some(())
}

/// Calls an original handler with `args` as its arguments, on this script's stack.
unsafe fn call_with(l: usize, handler: usize, args: &[HksValue]) -> Option<i32> {
    let top = memory::read_u64(l + 0x48)?;
    let base = memory::read_u64(l + 0x50)?;
    for a in args {
        if unsafe { push(l, a) }.is_none() {
            unsafe { std::ptr::write_volatile((l + 0x48) as *mut u64, top) };
            return None;
        }
    }
    unsafe {
        std::ptr::write_volatile((l + 0x50) as *mut u64, top);
        let f: unsafe extern "C" fn(usize) -> i32 = std::mem::transmute(handler);
        let r = f(l);
        std::ptr::write_volatile((l + 0x50) as *mut u64, base);
        std::ptr::write_volatile((l + 0x48) as *mut u64, top);
        Some(r)
    }
}

/// Compiles and runs `source` in this script state; its first return value.
unsafe fn eval(l: usize, source: &str) -> Result<HksValue, String> {
    let (loadbuffer, pcall) = eval_fns().ok_or("the HKS compiler wasn't found")?;
    let loadbuffer: unsafe extern "C" fn(usize, *const u8, usize, *const u8) -> i32 = unsafe { std::mem::transmute(loadbuffer) };
    let pcall: unsafe extern "C" fn(usize, i32, i32, i32) -> i32 = unsafe { std::mem::transmute(pcall) };
    let read = |at: usize| memory::read_u64(at).unwrap_or(0) as usize;
    let depth = read(l + 0x48).saturating_sub(read(l + 0x50)) / 16;
    let out = if unsafe { loadbuffer(l, source.as_ptr(), source.len(), CHUNK_NAME.as_ptr()) } != 0 {
        Err(string_value(l, -1).unwrap_or_else(|| "the chunk didn't compile".into()))
    } else if unsafe { pcall(l, 0, 1, 0) } != 0 {
        // The game's error handler appends a stack traceback: keep the message.
        let msg = string_value(l, -1).unwrap_or_else(|| "the chunk raised an error".into());
        Err(msg.split("\nstack traceback:").next().unwrap_or_default().to_string())
    } else {
        Ok(slot_value(l, read(l + 0x48) - 16, -1))
    };
    // The stack may have moved while the chunk ran: restore its depth, not the old pointer.
    unsafe { std::ptr::write_volatile((l + 0x48) as *mut u64, (read(l + 0x50) + depth * 16) as u64) };
    out
}

/// Runs queued requests for the character whose script is calling (from the hooks).
fn run_requests(l: usize) {
    if !HAS_REQUESTS.load(Ordering::Relaxed) {
        return;
    }
    let Some(owner) = script_owner(l) else { return };
    let mine: Vec<Request> = {
        let mut q = REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
        if !q.iter().any(|r| r.owner == owner) {
            return;
        }
        let (mine, rest): (Vec<Request>, Vec<Request>) = q.drain(..).partition(|r| r.owner == owner);
        *q = rest;
        HAS_REQUESTS.store(!q.is_empty(), Ordering::Relaxed);
        mine
    };
    for r in mine {
        let reply = match &r.kind {
            RequestKind::Eval(source) => match unsafe { eval(l, SANITIZE) } {
                Ok(HksValue::Bool(true)) => match unsafe { eval(l, source) } {
                    Ok(v) => Reply::Value(v),
                    Err(e) => Reply::Error(e),
                },
                _ => Reply::Error("the script's state could not be made safe to run code in".into()),
            },
            kind => {
                let handler = if matches!(kind, RequestKind::Act) { ACT_ORIGINAL.load(Ordering::Acquire) } else { ENV_ORIGINAL.load(Ordering::Acquire) };
                match (handler != 0).then(|| unsafe { call_with(l, handler, &r.args) }).flatten() {
                    Some(i) => Reply::Int(i),
                    None => Reply::Error("the call could not be made".into()),
                }
            }
        };
        RESULTS.lock().unwrap_or_else(|e| e.into_inner()).push((r.id, reply));
    }
}

// -- script -> Kindle: kindle(name, ...) ----------------------------------------------
//
// Kindle adds a global `kindle` function to every script state, right after the game
// registers `env` in it: a hook on the game's helper that registers one named C function as a
// global (+0x10AE8B0: push the closure, set the global; every registration function calls it).
// (It was a hook on the function registering env/isenv/act, found by a pattern that matches six
// such functions and picked by the "env" it names; the game keeps that one encrypted except
// while it runs, so it was missed at most starts: "pattern matched 6 times", no kindle().) An edited script can call
// `kindle("name", ...)`: mods' hks.on_call("name") handlers run there and then, on the
// script's thread, and the first value one returns goes back to the script. Scripts should
// check for it (`if kindle then ... end`) so they still work without Kindle.

/// The game's "register a named C function as a global" helper: (L, name, fn).
const REGISTER_NAME_PATTERN: &str = "48 89 5c 24 08 57 48 83 ec 30 49 8b c0 48 8b da 4c 8b ca 45 33 c0 48 8b d0 48 8b f9     c7 44 24 20 00 00 00 00 e8 ?? ?? ?? ?? 48 8b d3 48 8b cf 48 8b 5c 24 40 48 83 c4 30 5f e9";
static REGISTERED: AtomicUsize = AtomicUsize::new(0);

/// A script's kindle(name, ...) call, for mods' handlers.
pub struct ScriptCall {
    pub character: Option<usize>,
    pub name: String,
    pub args: Vec<HksValue>,
    /// What the script gets back (None: nothing).
    pub result: Option<HksValue>,
}

unsafe extern "C" fn kindle_fn(l: usize) -> i32 {
    let run = || {
        let mut a = args(l);
        if a.is_empty() {
            return 0;
        }
        let HksValue::Str(name) = a.remove(0) else { return 0 };
        let mut call = ScriptCall { character: script_owner(l), name, args: a, result: None };
        crate::dispatch_hks_call(&mut call);
        match call.result {
            Some(v) if unsafe { push(l, &v) }.is_some() => 1,
            _ => 0,
        }
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or(0)
}

fn registration_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (l, name, f) = unsafe { ((*reg).rcx as usize, (*reg).rdx as usize, (*reg).r8 as usize) };
    let register: unsafe extern "C" fn(usize, usize, usize) = unsafe { std::mem::transmute(original) };
    unsafe { register(l, name, f) };
    if l != 0 && memory::read_bytes(name, 4).is_some_and(|b| b == b"env\0") {
        unsafe { register(l, c"kindle".as_ptr() as usize, kindle_fn as *const () as usize) };
        if REGISTERED.fetch_add(1, Ordering::Relaxed) == 0 {
            crate::info!("hks: kindle() added to the behaviour scripts");
        }
    }
    0
}

static REGISTRATION_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "hks",
    name: "hks name registration",
    pattern: REGISTER_NAME_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(registration_hook),
    without: "scripts can't call kindle()",
};

/// Adds `kindle` to every script state created from now on (once, at startup).
pub fn install_bridge() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Some(found) = REGISTRATION_SITE.find() else { return };
        // Resolved now: the hook changes the bytes their patterns match, and this keeps the
        // lookups off the script threads.
        globals();
        push_fns();
        REGISTRATION_SITE.hook_all(&found);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanitize_refuses_precompiled_chunks() {
        let lua = mlua::Lua::new();
        // Lua 5.4 has no loadstring: the game's (5.1-style) one stands in as load.
        lua.load("loadstring = load").exec().unwrap();
        assert!(lua.load(super::SANITIZE).eval::<bool>().unwrap());
        assert!(lua.load(super::SANITIZE).eval::<bool>().unwrap(), "a second run wraps nothing again");
        assert!(lua.load("return loadstring('return 1 + 1')() == 2").eval::<bool>().unwrap());
        let dumped = "local f, e = loadstring(string.dump(function() end)) return f == nil and e ~= nil";
        assert!(lua.load(dumped).eval::<bool>().unwrap());
        assert!(lua.load("return load(function() return nil end) == nil").eval::<bool>().unwrap());
        assert!(lua.load("return io == nil and os == nil and debug == nil").eval::<bool>().unwrap());
        assert!(super::precompiled("\x1bLua") && !super::precompiled("return 1"));
    }
}
