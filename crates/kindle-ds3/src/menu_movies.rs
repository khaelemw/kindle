//! The game's menu movies that Kindle edits: read from the game's archive at the first load,
//! changed in code and served instead of the original through files.rs. No FromSoftware
//! file ships with Kindle, and a mod that replaces the same movie takes precedence.
//!
//! The pause menu (`menu/02_000_ingametop.gfx`, DS3 1.15.2): its root places `ItemList`, which
//! holds the top row `Item_0_0` … `Item_0_4` (98 units apart, each a sprite with its icon at
//! depth 1 and the shared `Cursor` at depth 3), the developers' sixth cell `Item_0_5` (296
//! units right, only a `Cursor`: the "Debug" entry front_menu.rs turns into "Mods"), the five
//! quick items `Item_1_*`, an unnamed separator and, at depth 1, the panel. The panel is one
//! shape: a 600 x 400 piece of the `MENU_Top` texture (atlas x 4..604, y 172..572, fill matrix
//! scale 20, translate (-304, -372)) with three bands: the frame around the top icons, the
//! frame around the quick items (atlas y 359..489, its lines at x 47..560) and a strip.
//! `MENU_Top` has a sixth, unused icon at atlas (504, 4), a skull.
//!
//! The edit: `Item_0_5` gets the skull (a copy of `Item_0_4`'s icon shape showing the atlas
//! square at x 504), the six cells are centred 98 apart, the separator is widened, and the
//! panel is rebuilt wider from pieces of its texture: the top band and the strip stretched,
//! the item frame at its own width so it still fits the items, its band's ends moved out to
//! the panel's edges and the gap filled with mirrored copies of a plain strip of the band
//! (atlas x 568..576). Splits are on the texture's dark rows. Verified in game 2026-10-04.

use crate::gfx::{self, Matrix, Movie, Place, Sprite, Tag};

/// Game path of a movie Kindle edits, and the edit.
pub struct MenuMovie {
    pub path: &'static str,
    pub edit: fn(&[u8]) -> Result<Vec<u8>, String>,
    /// What's lost if the edit can't apply, for the log.
    pub feature: &'static str,
}

pub const PAUSE_MENU: MenuMovie = MenuMovie { path: "menu/02_000_ingametop.gfx", edit: pause_menu, feature: "the pause menu's Mods icon" };
/// The character creation screen. The PC game loads its own copy (`menu/win/`, tried first);
/// both are edited the same way.
pub const CHAR_MAKE: MenuMovie =
    MenuMovie { path: "menu/04_000_chrmake_top.gfx", edit: char_make_top, feature: "the places of mods' rows on the character creation screen (it scrolls)" };
pub const CHAR_MAKE_PC: MenuMovie = MenuMovie {
    path: "menu/win/04_000_chrmake_top.gfx",
    edit: char_make_top,
    feature: "the places of mods' rows on the character creation screen (it scrolls)",
};

/// The System screen (Kindle's copies of it: the Mods screen, mods' creation screen pages); the PC
/// game's own copy too.
pub const OPTION_SETTING: MenuMovie =
    MenuMovie { path: "menu/02_040_optionsetting.gfx", edit: option_setting, feature: "the icons of Kindle's tabs on the game's screens" };
pub const OPTION_SETTING_PC: MenuMovie =
    MenuMovie { path: "menu/win/02_040_optionsetting.gfx", edit: option_setting, feature: "the icons of Kindle's tabs on the game's screens" };

/// The movies edited when the game menus' Mods entries are on.
pub fn movies() -> Vec<MenuMovie> {
    vec![PAUSE_MENU, CHAR_MAKE, CHAR_MAKE_PC, OPTION_SETTING, OPTION_SETTING_PC]
}

/// A tab icon of Kindle's: a piece of one of the game's menu textures (its name, pixel size,
/// and the piece's centre and size in pixels).
pub struct TabIcon {
    pub texture: &'static str,
    pub texture_size: (u16, u16),
    pub centre: (f64, f64),
    pub size: (f64, f64),
}

/// The travel screen's area pictures, in the System screen's gold style.
const WARP_ICONS: (&str, (u16, u16)) = ("MENU_WarpIcon", (1120, 96));
const fn warp(x0: f64, x1: f64) -> TabIcon {
    TabIcon { texture: WARP_ICONS.0, texture_size: WARP_ICONS.1, centre: ((x0 + x1) / 2.0, 48.0), size: (x1 - x0, 96.0) }
}

/// Kindle's tab icons, in order (KINDLE_ICON + k is icon k's frame in the tab icon sprites):
/// Kindle (a candelabra), Mods (a bell), skulls, the pause menu's Equipment picture, Firelink
/// Shrine (the last three for mods' pages).
pub const TAB_ICONS: [TabIcon; 5] = [
    warp(339.0, 382.0),
    warp(731.0, 790.0),
    warp(408.0, 477.0),
    TabIcon { texture: "MENU_Top", texture_size: (1024, 1024), centre: (49.0, 49.0), size: (90.0, 90.0) },
    warp(6.0, 74.0),
];
pub const KINDLE_TAB: usize = 0;
pub const MODS_TAB: usize = 1;
/// The icons' names, for mods' pages (kindle.ui.character_rows).
pub const TAB_ICON_NAMES: [&str; 5] = ["candelabra", "bell", "skulls", "equipment", "shrine"];

/// Whether the System screen the game loaded has Kindle's tab icons.
pub fn tab_icons_added() -> bool {
    served(&[OPTION_SETTING, OPTION_SETTING_PC])
}

/// Whether the character creation screen the game loaded has places for Kindle's rows.
pub fn char_make_places_added() -> bool {
    served(&[CHAR_MAKE, CHAR_MAKE_PC])
}

/// Whether the game got Kindle's edit of a movie (each copy it opened was edited).
fn served(copies: &[MenuMovie]) -> bool {
    let made: Vec<Option<bool>> = copies.iter().map(|m| crate::files::edit_made(m.path)).collect();
    made.contains(&Some(true)) && !made.contains(&Some(false))
}

/// The System screen (`menu/02_040_optionsetting.gfx`): a tab's icon is a sprite with a frame
/// per icon (8; 9 in one list of the non-PC copy), each a small sprite holding an 80 x 92 shape
/// that shows a piece of a texture; the game goes to the frame its tab names (native_mods.rs).
/// Two sprites list them: the tabs behind (`BackTabList`'s items' `ItemIcon`) and the
/// highlighted one (`TabList`'s items' `Cursor/ItemIcon`). The edit pads both to KINDLE_ICON
/// frames and adds a frame per Kindle icon, each a new sprite with a shape like the game's
/// showing a piece of `MENU_Top` or `MENU_WarpIcon` (declared in the movie like its own
/// textures, GFx tag 1009: u32 id, u16 format 13, u16 width, u16 height, name, file name).
pub fn option_setting(original: &[u8]) -> Result<Vec<u8>, String> {
    const ICON_BOUNDS: [i64; 4] = [-40 * 20, 40 * 20, -46 * 20, 46 * 20];
    const EXTERNAL_IMAGE: u16 = 1009;
    const REMOVE_OBJECT2: u16 = 28;
    const SHOW_FRAME: u16 = 1;
    let mut movie = Movie::parse(original)?;
    let back = movie.root_place("BackTabList").and_then(|p| p.id).ok_or("no BackTabList")?;
    let front = movie.root_place("TabList").and_then(|p| p.id).ok_or("no TabList")?;
    let item_of =
        |list: u16| -> Result<u16, String> { movie.sprite(list)?.place_named("Item_0_0").and_then(|(_, p)| p.id).ok_or_else(|| "no Item_0_0".to_string()) };
    let icon_list = |holder: u16, path: &[&str]| -> Result<u16, String> {
        let mut id = holder;
        for name in path {
            id = movie.sprite(id)?.place_named(name).and_then(|(_, p)| p.id).ok_or_else(|| format!("no {name}"))?;
        }
        Ok(id)
    };
    let lists = [icon_list(item_of(back)?, &["ItemIcon"])?, icon_list(item_of(front)?, &["Cursor", "ItemIcon"])?];
    for &list in &lists {
        if movie.sprite(list)?.frames as usize > KINDLE_ICON {
            return Err(format!("the tab icon sprite {list} has more than {KINDLE_ICON} frames"));
        }
    }
    // Texture ids: the movie's own declarations, plus MENU_WarpIcon.
    let declared = |movie: &Movie, name: &str| -> Option<u16> {
        movie.tags.iter().filter(|t| t.code == EXTERNAL_IMAGE && t.body.len() > 11).find_map(|t| {
            let n = t.body[10] as usize;
            (t.body.get(11..11 + n) == Some(name.as_bytes())).then(|| u16::from_le_bytes([t.body[0], t.body[1]]))
        })
    };
    let image_ids = movie.tags.iter().filter(|t| t.code == EXTERNAL_IMAGE && t.body.len() >= 2).map(|t| u16::from_le_bytes([t.body[0], t.body[1]]));
    let mut next = movie.max_id().max(image_ids.max().unwrap_or(0)) + 1;
    let mut new_tags = Vec::new();
    let texture = |movie: &Movie, name: &'static str, (w, h): (u16, u16), next: &mut u16, new_tags: &mut Vec<Tag>| -> u16 {
        if let Some(id) = declared(movie, name) {
            return id;
        }
        if let Some(t) = new_tags.iter().find(|t: &&Tag| t.code == EXTERNAL_IMAGE && t.body.get(11..11 + name.len()) == Some(name.as_bytes())) {
            return u16::from_le_bytes([t.body[0], t.body[1]]);
        }
        let id = *next;
        *next += 1;
        let mut body = (id as u32).to_le_bytes().to_vec();
        body.extend_from_slice(&13u16.to_le_bytes());
        body.extend_from_slice(&w.to_le_bytes());
        body.extend_from_slice(&h.to_le_bytes());
        let file = format!("{name}.tga");
        body.push(name.len() as u8);
        body.extend_from_slice(name.as_bytes());
        body.push(file.len() as u8);
        body.extend_from_slice(file.as_bytes());
        new_tags.push(Tag::new(EXTERNAL_IMAGE, body));
        id
    };
    let mut holders = Vec::new();
    for icon in &TAB_ICONS {
        let bitmap = texture(&movie, icon.texture, icon.texture_size, &mut next, &mut new_tags);
        let scale = (80.0 / icon.size.0).min(92.0 / icon.size.1).min(1.0);
        let fill = Matrix { sx: 20.0 * scale, sy: 20.0 * scale, r0: 0.0, r1: 0.0, tx: -icon.centre.0 * scale, ty: -icon.centre.1 * scale };
        let half = ((icon.size.0 * scale / 2.0 * 20.0) as i64, (icon.size.1 * scale / 2.0 * 20.0) as i64);
        let rect = [-half.0, half.0, -half.1, half.1];
        let (shape, holder) = (next, next + 1);
        next += 2;
        new_tags.push(Tag::new(gfx::DEFINE_SHAPE, gfx::rect_shape(shape, ICON_BOUNDS, &[(rect, (0x41, bitmap, fill))])));
        let sprite = Sprite {
            id: holder,
            frames: 1,
            tags: vec![Tag::new(gfx::PLACE_OBJECT2, Place::new_body(1, shape, &Matrix::IDENTITY)), Tag::new(SHOW_FRAME, vec![]), Tag::new(0, vec![])],
        };
        new_tags.push(Tag::new(gfx::DEFINE_SPRITE, sprite.to_body()));
        holders.push(holder);
    }
    // The new characters go before the first icon list that uses them.
    let first = lists.iter().filter_map(|&l| movie.find(l)).min().ok_or("no icon list")?;
    for (k, tag) in new_tags.into_iter().enumerate() {
        movie.tags.insert(first + k, tag);
    }
    for &list in &lists {
        let mut sprite = movie.sprite(list)?;
        // The icon's depth: the one the game's frames swap (the list behind also has a plate
        // under the icon and a shade over it).
        let depth = sprite
            .tags
            .iter()
            .find(|t| t.code == REMOVE_OBJECT2 && t.body.len() >= 2)
            .map(|t| u16::from_le_bytes([t.body[0], t.body[1]]))
            .ok_or("an icon list swaps nothing")?;
        let end = sprite.tags.iter().rposition(|t| t.code == 0).ok_or("an icon list has no end")?;
        // Up to KINDLE_ICON frames (the lists differ: 8 or 9), each still showing the last icon.
        let mut frames: Vec<Tag> = (sprite.frames as usize..KINDLE_ICON).map(|_| Tag::new(SHOW_FRAME, vec![])).collect();
        for &holder in &holders {
            frames.push(Tag::new(REMOVE_OBJECT2, depth.to_le_bytes().to_vec()));
            frames.push(Tag::new(gfx::PLACE_OBJECT2, Place::new_body(depth, holder, &Matrix::IDENTITY)));
            frames.push(Tag::new(SHOW_FRAME, vec![]));
        }
        sprite.tags.splice(end..end, frames);
        sprite.frames = (KINDLE_ICON + holders.len()) as u16;
        movie.set_sprite(&sprite)?;
    }
    Ok(movie.to_bytes())
}

/// The first frame index of Kindle's tab icons (the game's come first: 8 or 9 frames, padded
/// to this).
pub const KINDLE_ICON: usize = 16;

/// The character creation screen (`menu/04_000_chrmake_top.gfx`): its root places `ItemList`,
/// whose rows are `Item_0_0` … `Item_7_0` (50 units apart, y -305 to 45) and whose last item,
/// `Item_8_0` (y 195, another sprite), is the "Finalize creation" button. The screen's list (a
/// `GridControl`) sizes itself from the movie: it looks up `Item_<row>_<column>` in `ItemList`
/// until a row is missing (1.15.2 +0xA40500), so it has nine places (eight rows and the
/// button), shows the items in order in them and scrolls the rest. Kindle's rows (char_make.rs)
/// therefore each need a place: copies of `Item_7_0` 50 units apart below it become `Item_8_0`
/// on, and the button takes the name after them, moved down to keep its gap. Without them the
/// list scrolls, and the button then shows a row's item.
pub fn char_make_top(original: &[u8]) -> Result<Vec<u8>, String> {
    const PITCH: f64 = 50.0;
    /// A placement with a sprite, a matrix and a name, nothing else.
    const PLACED_NAMED: u8 = 0x26;
    let rows = crate::char_make::ROWS;
    let mut movie = Movie::parse(original)?;
    let list_id = movie.root_place("ItemList").and_then(|p| p.id).ok_or("no ItemList")?;
    let mut list = movie.sprite(list_id)?;
    let (last_at, last) = list.place_named("Item_7_0").ok_or("no Item_7_0")?;
    let (button_at, button) = list.place_named("Item_8_0").ok_or("no Item_8_0")?;
    let button_name = format!("Item_{}_0", 8 + rows);
    if list.place_named(&button_name).is_some() {
        return Err(format!("it already has an {button_name}"));
    }
    let row = last.id.ok_or("Item_7_0 places nothing")?;
    let m = last.matrix.ok_or("Item_7_0 has no matrix")?;
    let (Some(button_id), Some(bm), Some(&PLACED_NAMED)) = (button.id, button.matrix, list.tags[button_at].body.first()) else {
        return Err("Item_8_0's placement isn't a plain named one".into());
    };
    let depth = list.places().map(|(_, p)| p.depth).max().unwrap_or(0) + 10;
    // The button moves down as far as the rows grew past one (it keeps its gap) and takes the
    // name after them.
    let moved = Matrix { ty: bm.ty + (rows - 1) as f64 * PITCH, ..bm };
    list.tags[button_at].body = Place::new_named_body(button.depth, button_id, &moved, &button_name);
    for k in 0..rows {
        let place = Place::new_named_body(depth + 10 * k as u16, row, &Matrix { ty: m.ty + (k + 1) as f64 * PITCH, ..m }, &format!("Item_{}_0", 8 + k));
        list.tags.insert(last_at + 1 + k, Tag::new(gfx::PLACE_OBJECT2, place));
    }
    movie.set_sprite(&list)?;
    Ok(movie.to_bytes())
}

/// Atlas x of the unused skull icon in `MENU_Top`.
const SKULL_X: f64 = 504.0;
/// Icons are 90 units square, 98 apart.
const ICON_SIZE: f64 = 90.0;
const ICON_PITCH: f64 = 98.0;
const CELLS: usize = 6;
/// The panel's original half width and its margin past the outer icons.
const PANEL_HALF: f64 = 300.0;
const PANEL_MARGIN: f64 = 59.0;
/// The panel's fill: atlas origin of the panel's centre.
const PANEL_ORIGIN: (f64, f64) = (304.0, 372.0);
/// Atlas rows splitting the panel's bands, and its edges.
const PANEL_TOP: f64 = 172.0;
const ITEM_BAND: (f64, f64) = (359.0, 489.0);
const PANEL_BOTTOM: f64 = 572.0;
const PANEL_LEFT: f64 = 4.0;
const PANEL_RIGHT: f64 = 604.0;
/// Atlas columns: the item band's ends (before / after its frame) and the plain filler strip.
const ITEM_FRAME: (f64, f64) = (43.0, 565.0);
const FILLER: (f64, f64) = (568.0, 576.0);
const FILLER_COPIES: usize = 6;

/// The pause menu with a sixth icon in a wider panel (see the module docs).
pub fn pause_menu(original: &[u8]) -> Result<Vec<u8>, String> {
    let mut movie = Movie::parse(original)?;
    let list_id = movie.root_place("ItemList").and_then(|p| p.id).ok_or("no ItemList")?;
    let list = movie.sprite(list_id)?;
    let cell_id = |name: &str| list.place_named(name).and_then(|(_, p)| p.id).ok_or_else(|| format!("no {name}"));
    let sixth = cell_id("Item_0_5")?;
    let icon =
        movie.shape_under(movie.sprite(cell_id("Item_0_4")?)?.places().find(|(_, p)| p.depth == 1).and_then(|(_, p)| p.id).ok_or("Item_0_4 has no icon")?)?;
    let panel_holder = list.places().find(|(_, p)| p.depth == 1).and_then(|(_, p)| p.id).ok_or("ItemList has no panel")?;
    let panel = movie.shape_under(panel_holder)?;

    // The panel must be the texture piece this edit was made for.
    let panel_tag = movie.find(panel).map(|i| &movie.tags[i]).ok_or("no panel shape")?;
    if panel_tag.code != gfx::DEFINE_SHAPE {
        return Err("the panel isn't a plain shape".into());
    }
    let shape = gfx::bitmap_shape(&panel_tag.body)?;
    let [(fill_kind, bitmap, m)] = shape.fills[..] else { return Err("the panel isn't one bitmap".into()) };
    if shape.bounds != [-6000, 6000, -4000, 4000] || (m.sx, m.sy, m.tx, m.ty) != (20.0, 20.0, -PANEL_ORIGIN.0, -PANEL_ORIGIN.1) {
        return Err("the panel isn't the texture piece Kindle knows".into());
    }
    let xs: Vec<f64> = (0..CELLS).map(|i| (i as f64 - (CELLS - 1) as f64 / 2.0) * ICON_PITCH).collect();
    let widen = (xs[CELLS - 1] + ICON_SIZE / 2.0 + PANEL_MARGIN) / PANEL_HALF;

    // The skull: a copy of Item_0_4's icon showing another square of the atlas.
    let skull = movie.max_id() + 1;
    let icon_tag = movie.find(icon).map(|i| movie.tags[i].clone()).ok_or("no icon shape")?;
    let skull_tag = Tag::new(icon_tag.code, with_fill_translate(&icon_tag.body, skull, -(SKULL_X + ICON_SIZE / 2.0), -49.0)?);
    let mut cell = movie.sprite(sixth)?;
    let (cursor_at, _) = cell.place_named("Cursor").ok_or("Item_0_5 has no Cursor")?;
    cell.tags[cursor_at].body = gfx::with_depth(&cell.tags[cursor_at].body, 3);
    cell.tags.insert(cursor_at, Tag::new(gfx::PLACE_OBJECT2, Place::new_body(1, skull, &Matrix::IDENTITY)));
    movie.set_sprite(&cell)?;
    let at = movie.find(sixth).ok_or("no Item_0_5 sprite")?;
    movie.tags.insert(at, skull_tag);

    // The cells centred, the separator widened.
    let mut list = movie.sprite(list_id)?;
    let places: Vec<(usize, Place)> = list.places().collect();
    for (i, p) in places {
        let Some(m) = p.matrix else { continue };
        if let Some(k) = p.name.as_deref().and_then(|n| n.strip_prefix("Item_0_")).and_then(|k| k.parse::<usize>().ok())
            && k < CELLS
        {
            list.tags[i].body = gfx::with_matrix(&list.tags[i].body, &Matrix { tx: xs[k], ..m })?;
        } else if p.name.is_none() && p.depth != 1 {
            list.tags[i].body = gfx::with_matrix(&list.tags[i].body, &Matrix { sx: m.sx * widen, ..m })?;
        }
    }
    movie.set_sprite(&list)?;

    let i = movie.find(panel).ok_or("no panel shape")?;
    movie.tags[i].body = panel_shape(panel, widen, fill_kind, bitmap);
    Ok(movie.to_bytes())
}

/// A copy of a one-bitmap DefineShape with a new id and its fill showing from (tx, ty).
fn with_fill_translate(body: &[u8], id: u16, tx: f64, ty: f64) -> Result<Vec<u8>, String> {
    let (_, q) = gfx::read_rect(body, 2).ok_or("icon shape cut short")?;
    if body.get(q) != Some(&1) || !(0x40..=0x43).contains(body.get(q + 1).unwrap_or(&0)) {
        return Err("the icon isn't one bitmap".into());
    }
    let (m, end) = gfx::read_matrix(body, q + 4).ok_or("icon shape cut short")?;
    let mut out = id.to_le_bytes().to_vec();
    out.extend_from_slice(&body[2..q + 4]);
    out.extend(gfx::write_matrix(&Matrix { tx, ty, ..m }));
    out.extend_from_slice(&body[end..]);
    Ok(out)
}

/// The panel rebuilt `widen` times wider (see the module docs).
fn panel_shape(id: u16, widen: f64, kind: u8, bitmap: u16) -> Vec<u8> {
    let half = PANEL_HALF * widen;
    let extra = half - PANEL_HALF;
    let end_width = ITEM_FRAME.0 - PANEL_LEFT;
    let frame_half = (ITEM_FRAME.1 - ITEM_FRAME.0) / 2.0;
    // (atlas x0, x1, y0, y1, panel x0, x1, mirrored)
    let mut pieces = vec![(PANEL_LEFT, PANEL_RIGHT, PANEL_TOP, ITEM_BAND.0, -half, half, false)];
    pieces.push((PANEL_LEFT, ITEM_FRAME.0, ITEM_BAND.0, ITEM_BAND.1, -half, -half + end_width, false));
    let copy = extra / FILLER_COPIES as f64;
    for start in [-half + end_width, frame_half] {
        for i in 0..FILLER_COPIES {
            let x0 = start + i as f64 * copy;
            pieces.push((FILLER.0, FILLER.1, ITEM_BAND.0, ITEM_BAND.1, x0, x0 + copy, i % 2 == 1));
        }
    }
    pieces.push((ITEM_FRAME.0, ITEM_FRAME.1, ITEM_BAND.0, ITEM_BAND.1, -frame_half, frame_half, false));
    pieces.push((ITEM_FRAME.1, PANEL_RIGHT, ITEM_BAND.0, ITEM_BAND.1, half - end_width, half, false));
    pieces.push((PANEL_LEFT, PANEL_RIGHT, ITEM_BAND.1, PANEL_BOTTOM, -half, half, false));

    let twips = |v: f64| (v * gfx::TWIPS).round() as i64;
    let rects: Vec<([i64; 4], gfx::BitmapFill)> = pieces
        .iter()
        .map(|&(ax0, ax1, ay0, ay1, x0, x1, mirrored)| {
            let k = (x1 - x0) / (ax1 - ax0);
            let (sx, tx) = if mirrored { (-k, x0 + ax1 * k) } else { (k, x0 - ax0 * k) };
            let fill = Matrix { sx: sx * gfx::TWIPS, sy: gfx::TWIPS, r0: 0.0, r1: 0.0, tx, ty: -PANEL_ORIGIN.1 };
            ([twips(x0), twips(x1), twips(ay0 - PANEL_ORIGIN.1), twips(ay1 - PANEL_ORIGIN.1)], (kind, bitmap, fill))
        })
        .collect();
    gfx::rect_shape(id, [-twips(half), twips(half), -4000, 4000], &rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_covers_its_width_without_gaps() {
        let body = panel_shape(21, 349.0 / 300.0, 0x40, 1);
        let shape = gfx::bitmap_shape(&body).unwrap();
        assert_eq!(shape.bounds, [-6980, 6980, -4000, 4000]);
        assert_eq!(shape.fills.len(), 5 + 2 * FILLER_COPIES);
        // The item frame keeps its own scale and atlas position.
        assert!(shape.fills.iter().any(|(_, _, m)| (m.sx, m.tx) == (20.0, -PANEL_ORIGIN.0)));
    }
}
