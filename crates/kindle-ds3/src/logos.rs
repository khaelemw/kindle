//! The logos at the start of the game (Bandai Namco, FromSoftware), skipped unless kindle.toml
//! says otherwise: the title comes as soon as the game can show it.
//!
//! A logo screen's update (+0xBF6410 in 1.15.2) adds the frame time to its timer (+0x9D8) and
//! moves on when the timer reaches the logo's length (+0x9DC), or earlier on a button press
//! once `MenuMan.LogoSkipEnableTime` (1 s) has passed. Kindle sets the timer to the length
//! before each update runs, so each logo ends on its first frame.

const LOGO_UPDATE: &str = "40 55 56 57 48 81 ec 80 00 00 00 48 c7 44 24 28 fe ff ff ff 48 89 9c 24 b8 00 00 00 0f 29 74 24 70 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 44 24 60 49 8b f0 0f 28 f1 48 8b f9 8b 05 ?? ?? ?? ?? a8 01";
const TIMER: usize = 0x9D8;
const LENGTH: usize = 0x9DC;

static LOGO_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "logos",
    name: "logo screen",
    pattern: LOGO_UPDATE,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::JmpBack(before_update),
    without: "the logos play",
};

pub fn install() {
    LOGO_SITE.install();
}

fn before_update(reg: *mut ilhook::x64::Registers) {
    let screen = unsafe { (*reg).rcx } as usize;
    if let Some(length) = crate::memory::read::<f32>(screen + LENGTH)
        && length.is_finite()
        && length > 0.0
    {
        unsafe { std::ptr::write_volatile((screen + TIMER) as *mut f32, length) };
    }
}
