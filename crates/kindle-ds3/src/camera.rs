//! A camera a mod sets (kindle.player.set_camera).
//!
//! The game fills its gameplay camera (SprjCamera `+0x08`, the one it renders with and
//! game::camera reads) every frame by copying another of its cameras into it: `+0x5103E0(dst,
//! src)` (1.15.2, unique pattern) copies fov, aspect, near and far (`+0x50..+0x5C`), then the
//! matrix rows right, up, forward and position (`+0x10..+0x4C`, four f32 each); `+0x989F90`
//! calls it each frame. While a mod holds the camera, that copy into the gameplay camera takes
//! the mod's camera instead: the game's own cameras (follow, lock-on) go on as they were, and
//! come back the frame the mod lets go.

use std::sync::{Mutex, Once};

const COPY_PATTERN: &str =
    "40 53 48 83 ec 60 8b 42 50 4c 8b c2 48 8b d9 89 41 50 8b 42 54 89 41 54 8b 42 58 89 41 58 8b 42 5c 48 8d 54 24 20 89 41 5c 49 8b c8 e8";

/// The mod's camera: where it is, the way it looks (unit length) and its vertical field of view
/// (None: the game's).
struct Held {
    owner: String,
    pos: [f32; 3],
    forward: [f32; 3],
    fov: Option<f32>,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);
static HOOKED: Once = Once::new();
static HOOK_ERROR: Mutex<Option<String>> = Mutex::new(None);

static COPY_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "camera",
    name: "camera copy",
    pattern: COPY_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(copy_hook),
    without: "mods can't set the camera",
};

fn held() -> std::sync::MutexGuard<'static, Option<Held>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (len > 1e-6 && len.is_finite()).then(|| [v[0] / len, v[1] / len, v[2] / len])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The camera's right and up for a look direction, as the game's own are: right = world up x
/// forward, up = forward x right (looking straight up or down: right is world x).
pub fn basis(forward: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let right = normalize(cross([0.0, 1.0, 0.0], forward)).unwrap_or([1.0, 0.0, 0.0]);
    (right, cross(forward, right))
}

/// `owner` holds the camera at `pos`, looking along `forward` (any length), with `fov` (radians)
/// or the game's. Another mod's hold is an error.
pub fn hold(owner: &str, pos: [f32; 3], forward: [f32; 3], fov: Option<f32>) -> Result<(), String> {
    let forward = normalize(forward).ok_or("the look direction has no length")?;
    if pos.iter().any(|v| !v.is_finite()) {
        return Err("the position isn't a number".into());
    }
    let fov = fov.map(|f| f.clamp(0.05, 3.0));
    HOOKED.call_once(|| {
        if let Err(e) = COPY_SITE.install_or_why() {
            *HOOK_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
        }
    });
    if let Some(e) = HOOK_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Err(e);
    }
    let mut h = held();
    if let Some(other) = h.as_ref().filter(|h| h.owner != owner) {
        return Err(format!("{} holds the camera", other.owner));
    }
    *h = Some(Held { owner: owner.into(), pos, forward, fov });
    Ok(())
}

/// `owner` lets the camera go (also when it unloads); whether it held it.
pub fn release(owner: &str) -> bool {
    let mut h = held();
    if h.as_ref().is_some_and(|h| h.owner == owner) {
        *h = None;
        true
    } else {
        false
    }
}

fn copy_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (dst, src) = unsafe { ((*reg).rcx as usize, (*reg).rdx as usize) };
    let f: unsafe extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    let result = unsafe { f(dst, src) };
    let h = held();
    if let Some(h) = h.as_ref()
        && crate::game::gameplay_camera() == Some(dst)
    {
        let (right, up) = basis(h.forward);
        let rows = [(0x10, right, 0.0), (0x20, up, 0.0), (0x30, h.forward, 0.0), (0x40, h.pos, 1.0)];
        for (at, v, w) in rows {
            for (i, x) in v.into_iter().chain([w]).enumerate() {
                // dst is the game's camera the original call just wrote.
                unsafe { std::ptr::write_unaligned((dst + at + i * 4) as *mut f32, x) };
            }
        }
        if let Some(fov) = h.fov {
            unsafe { std::ptr::write_unaligned((dst + 0x50) as *mut f32, fov) };
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basis_matches_the_games() {
        // Read from the game: forward (-0.965, 0, -0.261) had right (-0.261, 0, 0.965), up (0, 1, 0).
        let (right, up) = basis([-0.965, 0.0, -0.261]);
        assert!((right[0] + 0.261).abs() < 1e-3 && right[1].abs() < 1e-6 && (right[2] - 0.965).abs() < 1e-3, "{right:?}");
        assert!(up[0].abs() < 1e-3 && (up[1] - 1.0).abs() < 1e-3 && up[2].abs() < 1e-3, "{up:?}");
        // Straight down: still a basis.
        let (right, up) = basis([0.0, -1.0, 0.0]);
        assert_eq!(right, [1.0, 0.0, 0.0]);
        assert!((up[2] + 1.0).abs() < 1e-6 || (up[2] - 1.0).abs() < 1e-6, "{up:?}");
    }
}
