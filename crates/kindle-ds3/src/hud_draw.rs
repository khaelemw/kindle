//! Draws mods' layers (kindle.ui.layer, kindle_core::hud) on the render thread: under every
//! window and menu Kindle draws, hidden on loading screens and while the screen is faded out.

use std::collections::HashMap;
use std::sync::Mutex;

use hudhook::RenderContext;
use hudhook::imgui::{FontId, ImColor32, TextureId, Ui, sys};
use kindle_core::hud::{self, Align, Font, Shape};

/// The pictures' textures, by image id (kindle_core::hud::Image), made on the render thread.
static TEXTURES: Mutex<Option<HashMap<u64, TextureId>>> = Mutex::new(None);

/// Makes the textures of pictures mods loaded (for layers and screens) that don't have one yet
/// (before a frame renders: hudhook's renderer makes textures then). A picture that fails is
/// logged and skipped.
pub fn upload(rc: &mut dyn RenderContext) {
    let mut textures = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let textures = textures.get_or_insert_with(HashMap::new);
    for image in hud::images() {
        if textures.contains_key(&image.id) {
            continue;
        }
        match rc.load_texture(&image.rgba, image.width, image.height) {
            Ok(id) => {
                textures.insert(image.id, id);
            }
            Err(err) => {
                crate::warn!("a mod's picture ({} x {}): no texture: {err:?}", image.width, image.height);
                textures.insert(image.id, TextureId::new(usize::MAX));
            }
        }
    }
}

/// A picture's texture, once upload made it.
pub fn texture(image: u64) -> Option<TextureId> {
    TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&image).copied().filter(|t| t.id() != usize::MAX)
}

/// Draws the shown layers. `serif` is the overlay's banner serif (None: the default font).
pub fn draw(ui: &Ui, serif: Option<FontId>, fade: f32) {
    if !crate::ui_bridge::IN_WORLD.load(std::sync::atomic::Ordering::Relaxed) || fade > 0.3 {
        return;
    }
    let layers = hud::shown();
    if layers.is_empty() {
        return;
    }
    let size = ui.io().display_size;
    let (s, origin) = hud::canvas_fit(size);
    let cam = hud::camera();
    // The fonts' ImFont: the default (no font pushed here), and the serif while pushed.
    let sans = unsafe { sys::igGetFont() };
    let serif = serif
        .map(|f| {
            let _t = ui.push_font(f);
            unsafe { sys::igGetFont() }
        })
        .unwrap_or(sans);
    let dl = ui.get_background_draw_list();
    let raw = unsafe { sys::igGetBackgroundDrawList() };
    for layer in &layers {
        for e in &layer.elements {
            let [x, y] = match e.at {
                None => [origin[0] + e.x * s, origin[1] + e.y * s],
                Some(point) => {
                    let Some(cam) = &cam else { continue };
                    let Some((at, distance)) = hud::project(cam, point, size) else { continue };
                    if e.range.is_some_and(|r| distance > r) {
                        continue;
                    }
                    [at[0] + e.x * s, at[1] + e.y * s]
                }
            };
            match &e.shape {
                Shape::Text { text, size: px, font, align, shadow } => {
                    let font = if *font == Font::Serif { serif } else { sans };
                    let px = px * s;
                    let range = text.as_bytes().as_ptr_range();
                    let (begin, end) = (range.start as *const _, range.end as *const _);
                    let mut measured = sys::ImVec2 { x: 0.0, y: 0.0 };
                    unsafe { sys::ImFont_CalcTextSizeA(&mut measured, font, px, f32::MAX, 0.0, begin, end, std::ptr::null_mut()) };
                    let tx = match align {
                        Align::Left => x,
                        Align::Center => x - measured.x / 2.0,
                        Align::Right => x - measured.x,
                    };
                    let put = |dx: f32, color: [f32; 4]| unsafe {
                        sys::ImDrawList_AddText_FontPtr(
                            raw,
                            font,
                            px,
                            sys::ImVec2 { x: tx + dx, y: y + dx },
                            ImColor32::from(color).to_bits(),
                            begin,
                            end,
                            0.0,
                            std::ptr::null(),
                        )
                    };
                    if *shadow {
                        put((px / 18.0).max(1.0), [0.0, 0.0, 0.0, e.color[3] * 0.85]);
                    }
                    put(0.0, e.color);
                }
                Shape::Rect { w, h, filled, rounding, thickness } => {
                    dl.add_rect([x, y], [x + w * s, y + h * s], e.color).filled(*filled).rounding(rounding * s).thickness(thickness * s).build();
                }
                Shape::Line { dx, dy, thickness } => {
                    dl.add_line([x, y], [x + dx * s, y + dy * s], e.color).thickness(thickness * s).build();
                }
                Shape::Bar { w, h, value, back } => {
                    let (w, h) = (w * s, h * s);
                    dl.add_rect([x, y], [x + w, y + h], *back).filled(true).build();
                    if *value > 0.0 {
                        dl.add_rect([x, y], [x + w * value, y + h], e.color).filled(true).build();
                    }
                }
                Shape::Circle { r, filled, thickness } => {
                    dl.add_circle([x, y], r * s, e.color).filled(*filled).thickness(thickness * s).build();
                }
                Shape::Image { image, w, h } => {
                    if let Some(tex) = texture(image.id) {
                        dl.add_image(tex, [x, y], [x + w * s, y + h * s]).col(e.color).build();
                    }
                }
            }
        }
    }
}
