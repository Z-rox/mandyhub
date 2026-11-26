//! Mandyhub's look in one place, after GTA VI's own site: a deep navy night,
//! a pastel sunset strip along the top, soft pink pill buttons with plum
//! type, lavender spaced capitals for labels and pale blush for reading.
//! Rockstar's typeface is theirs, so the look-alikes are free ones built into
//! the exe (fonts/, SIL Open Font License): Josefin Sans for spaced capitals
//! and buttons, Big Shoulders Display for big names and figures. Reading
//! text and icons are Windows' own (Segoe UI, Segoe Fluent Icons, Cascadia).
//! Hover steps in over 150 ms and out over 120 ms, a press sinks 1 px,
//! indicators slide in 240 ms.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, FontFamily, FontId, Frame, Galley, Id, Margin, Pos2, Rect,
    Response, RichText, Rounding, Sense, Shadow, Stroke, TextStyle, Ui,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

// The night, from the site's own grounds and panels.
pub const BG: Color32 = Color32::from_rgb(0x0c, 0x0d, 0x1b);
/// The list and the status bar: a step up from the ground.
pub const DEEP: Color32 = Color32::from_rgb(0x10, 0x11, 0x24);
const NIGHT_HI: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x38);
pub const SURFACE: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x34);
const SURFACE_LOW: Color32 = Color32::from_rgb(0x15, 0x16, 0x2b);
pub const RAISED: Color32 = Color32::from_rgb(0x2f, 0x2e, 0x52);
const RAISED_HI: Color32 = Color32::from_rgb(0x38, 0x36, 0x5d);
const FIELD: Color32 = Color32::from_rgb(0x14, 0x15, 0x2b);
pub const LINE: Color32 = Color32::from_rgb(0x30, 0x30, 0x55);
pub const LINE_STRONG: Color32 = Color32::from_rgb(0x46, 0x44, 0x72);
const LINE_HI: Color32 = Color32::from_rgb(0x55, 0x51, 0x86);
// Type.
pub const WHITE: Color32 = Color32::WHITE;
/// The site's reading colour: pale blush.
pub const BLUSH: Color32 = Color32::from_rgb(0xfb, 0xd9, 0xe0);
/// The site's small headings.
pub const LILAC: Color32 = Color32::from_rgb(0xcc, 0xc2, 0xf5);
pub const MUTED: Color32 = Color32::from_rgb(0xc6, 0xc1, 0xe2);
pub const FAINT: Color32 = Color32::from_rgb(0xa7, 0xa2, 0xc9);
// The sunset.
pub const PINK: Color32 = Color32::from_rgb(0xff, 0xbe, 0xcf);
const PINK_HI: Color32 = Color32::from_rgb(0xff, 0xd3, 0xde);
/// Type on pink and on the strip.
pub const PLUM: Color32 = Color32::from_rgb(0x43, 0x31, 0x52);
const PEACH: Color32 = Color32::from_rgb(0xff, 0xbd, 0xad);
const MAUVE: Color32 = Color32::from_rgb(0xce, 0x8b, 0xb5);
const LAVENDER: Color32 = Color32::from_rgb(0x91, 0x6f, 0xb4);
// Status, in the same pastel key.
pub const GOOD: Color32 = Color32::from_rgb(0x8e, 0xe3, 0xc8);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xd0, 0x8a);
pub const BAD: Color32 = Color32::from_rgb(0xff, 0x8f, 0x8f);
const ON_BAD: Color32 = Color32::from_rgb(0x2a, 0x0c, 0x12);

pub const R: f32 = 10.0;
pub const R_LG: f32 = 16.0;
/// The height of every field and normal button.
pub const CONTROL_H: f32 = 34.0;

/// Segoe Fluent Icons (Windows 11) and Segoe MDL2 Assets (Windows 10) share these.
pub mod glyph {
    pub const ADD: char = '\u{E710}';
    pub const CHECK: char = '\u{E73E}';
    pub const COPY: char = '\u{E8C8}';
    pub const DELETE: char = '\u{E74D}';
    pub const DOWNLOAD: char = '\u{E896}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const LINK: char = '\u{E71B}';
    pub const OPEN: char = '\u{E8A7}';
    pub const PLAY: char = '\u{E768}';
    pub const REFRESH: char = '\u{E72C}';
    pub const REPAIR: char = '\u{E90F}';
    pub const SEARCH: char = '\u{E721}';
    pub const SETTINGS: char = '\u{E713}';
    pub const SYNC: char = '\u{E895}';
    pub const WARNING: char = '\u{E7BA}';
    pub const STOP: char = '\u{E71A}';
    pub const WARM: char = '\u{E945}';
}

/// Whether an icon font was found (without one, controls show words only).
static ICONS: AtomicBool = AtomicBool::new(false);

pub fn has_icons() -> bool {
    ICONS.load(Ordering::Relaxed)
}

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}
/// Reading text: Segoe UI.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
/// Names in lists: Segoe UI Semibold.
pub fn strong(size: f32) -> FontId {
    FontId::new(size, family("strong"))
}
/// Big condensed names and figures: Big Shoulders Display ExtraBold.
pub fn display(size: f32) -> FontId {
    FontId::new(size, family("display"))
}
/// Spaced capitals: Josefin Sans Bold.
pub fn deco(size: f32) -> FontId {
    FontId::new(size, family("deco"))
}
/// Buttons: Josefin Sans SemiBold.
pub fn deco_semi(size: f32) -> FontId {
    FontId::new(size, family("deco-semi"))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
pub fn glyphs(size: f32) -> FontId {
    FontId::new(size, family("icons"))
}

/// The bundled faces, then Windows' own, each with egui's behind it for
/// anything it lacks.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Fonts");
    let mut load = |key: &str, files: &[&str]| {
        for f in files {
            if let Ok(bytes) = std::fs::read(dir.join(f)) {
                fonts.font_data.insert(key.to_owned(), egui::FontData::from_owned(bytes));
                return true;
            }
        }
        false
    };
    let has_text = load("text", &["segoeui.ttf"]);
    let has_strong = load("strong", &["seguisb.ttf", "segoeuib.ttf"]);
    let has_mono = load("mono", &["CascadiaMono.ttf", "consola.ttf"]);
    let has_glyphs = load("icons", &["SegoeIcons.ttf", "segmdl2.ttf"]);
    let bundled: [(&str, &'static [u8]); 3] = [
        ("deco", include_bytes!("../fonts/JosefinSans-Bold.ttf")),
        ("deco-semi", include_bytes!("../fonts/JosefinSans-SemiBold.ttf")),
        ("display", include_bytes!("../fonts/BigShouldersDisplay-ExtraBold.ttf")),
    ];
    for (key, bytes) in bundled {
        fonts.font_data.insert(key.to_owned(), egui::FontData::from_static(bytes));
    }

    if has_text {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "text".to_owned());
    }
    if has_mono {
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".to_owned());
    }
    let text = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let chain = |first: Option<&str>, rest: &[String]| -> Vec<String> {
        first.map(str::to_owned).into_iter().chain(rest.iter().cloned()).collect()
    };
    let strong_chain = chain(has_strong.then_some("strong"), &text);
    for key in ["deco", "deco-semi", "display"] {
        fonts.families.insert(family(key), chain(Some(key), &strong_chain));
    }
    fonts.families.insert(family("strong"), strong_chain);
    fonts.families.insert(family("icons"), chain(has_glyphs.then_some("icons"), &text));
    ICONS.store(has_glyphs, Ordering::Relaxed);
    ctx.set_fonts(fonts);
}

/// The site's colour runs as small pictures that shapes are filled with,
/// which keeps rounded corners smooth: the sunset strip (across), the sky
/// (down, peach to lavender) and the panel light (down, a lit top).
static STRIP: OnceLock<egui::TextureHandle> = OnceLock::new();
static SKY: OnceLock<egui::TextureHandle> = OnceLock::new();
static PANEL: OnceLock<egui::TextureHandle> = OnceLock::new();
const STRIP_STOPS: [(f32, Color32); 4] = [
    (0.0, Color32::from_rgb(0xff, 0xc9, 0xb8)),
    (0.38, Color32::from_rgb(0xfb, 0xb7, 0xcb)),
    (0.72, Color32::from_rgb(0xd9, 0x9b, 0xc4)),
    (1.0, Color32::from_rgb(0xa8, 0x87, 0xcc)),
];
const SKY_STOPS: [(f32, Color32); 3] = [(0.0, PEACH), (0.5, MAUVE), (1.0, LAVENDER)];
const PANEL_STOPS: [(f32, Color32); 2] = [(0.0, SURFACE), (1.0, SURFACE_LOW)];

#[derive(Clone, Copy)]
pub enum Fill {
    Strip,
    Sky,
    Panel,
}

fn sample(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        if t <= w[1].0 {
            return mix(w[0].1, w[1].1, (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6));
        }
    }
    stops.last().map_or(PINK, |s| s.1)
}

fn gradient_image(len: usize, stops: &[(f32, Color32)], down: bool) -> egui::ColorImage {
    let pixels = (0..len).map(|i| sample(stops, i as f32 / (len - 1) as f32)).collect();
    egui::ColorImage { size: if down { [1, len] } else { [len, 1] }, pixels }
}

/// The night theme, always (it does not follow the system's light mode).
pub fn apply(ctx: &egui::Context) {
    let linear = egui::TextureOptions::LINEAR;
    let _ = STRIP.set(ctx.load_texture("strip", gradient_image(512, &STRIP_STOPS, false), linear));
    let _ = SKY.set(ctx.load_texture("sky", gradient_image(256, &SKY_STOPS, true), linear));
    let _ = PANEL.set(ctx.load_texture("panel", gradient_image(256, &PANEL_STOPS, true), linear));
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        let v = &mut s.visuals;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.window_stroke = Stroke::new(1.0, LINE_STRONG);
        v.window_rounding = Rounding::same(R_LG);
        v.window_shadow = shadow();
        v.popup_shadow = Shadow { offset: vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(140) };
        v.menu_rounding = Rounding::same(R);
        v.extreme_bg_color = FIELD;
        v.faint_bg_color = SURFACE;
        v.code_bg_color = SURFACE;
        v.hyperlink_color = PINK;
        v.selection.bg_fill = PINK.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, PINK);
        v.text_cursor.stroke = Stroke::new(1.5, WHITE);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.weak_bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        w.inactive.bg_fill = RAISED;
        w.inactive.weak_bg_fill = RAISED;
        w.inactive.bg_stroke = Stroke::new(1.0, LINE_STRONG);
        w.inactive.fg_stroke = Stroke::new(1.0, WHITE);
        w.hovered.bg_fill = RAISED_HI;
        w.hovered.weak_bg_fill = RAISED_HI;
        w.hovered.bg_stroke = Stroke::new(1.0, LINE_HI);
        w.hovered.fg_stroke = Stroke::new(1.0, WHITE);
        w.hovered.expansion = 0.0;
        w.active.bg_fill = RAISED_HI;
        w.active.weak_bg_fill = RAISED_HI;
        w.active.bg_stroke = Stroke::new(1.0, PINK);
        w.active.fg_stroke = Stroke::new(1.0, WHITE);
        w.active.expansion = 0.0;
        w.open = w.hovered;
        for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            x.rounding = Rounding::same(R);
        }
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 6.0);
        s.spacing.interact_size = vec2(34.0, CONTROL_H);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.text_styles = [
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, deco_semi(14.0)),
            (TextStyle::Heading, display(28.0)),
            (TextStyle::Monospace, mono(12.5)),
        ]
        .into();
    });
}

fn shadow() -> Shadow {
    Shadow { offset: vec2(0.0, 20.0), blur: 56.0, spread: 0.0, color: Color32::from_rgba_unmultiplied(3, 3, 10, 200) }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The one entrance curve: fast out, soft landing.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 0 at rest, 1 hovered: 150 ms in, 120 ms out.
fn hover(ui: &Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time(id.with("hover"), on, if on { 0.15 } else { 0.12 })
}

/// Fills `rect` with one of the colour runs; `tint` white is full strength.
pub fn fill(painter: &egui::Painter, rect: Rect, rounding: impl Into<Rounding>, which: Fill, tint: Color32) {
    let rounding = rounding.into();
    let tex = match which {
        Fill::Strip => &STRIP,
        Fill::Sky => &SKY,
        Fill::Panel => &PANEL,
    };
    match tex.get() {
        Some(tex) => {
            let mut shape = egui::epaint::RectShape::filled(rect, rounding, tint);
            shape.fill_texture_id = tex.id();
            shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.add(shape);
        }
        None => {
            painter.rect_filled(rect, rounding, SURFACE);
        }
    }
}

/// The page's ground: the night a little lit at the top, as on the site.
pub fn ground(painter: &egui::Painter, rect: Rect) {
    let lit = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height().min(520.0)));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(lit.left_top(), NIGHT_HI);
    mesh.colored_vertex(lit.right_top(), NIGHT_HI);
    mesh.colored_vertex(lit.left_bottom(), BG);
    mesh.colored_vertex(lit.right_bottom(), BG);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

/// A round mark in the sky's colours.
pub fn mark(painter: &egui::Painter, center: Pos2, radius: f32) {
    fill(painter, Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0)), radius, Fill::Sky, Color32::WHITE);
}

/// The window's icon: the same round mark.
pub fn app_icon() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let c = (n as f32 - 1.0) / 2.0;
    let r = n as f32 * 0.47;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let edge = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            let col = sample(&SKY_STOPS, (y as f32 - (c - r)) / (2.0 * r));
            let i = (y * n + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[col.r(), col.g(), col.b(), (edge * 255.0) as u8]);
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

/// Spaced capitals, the way the site sets its labels.
pub fn caps(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(RichText::new(text.to_uppercase()).font(deco(size)).color(color).extra_letter_spacing(size * 0.16))
}

fn caps_galley(painter: &egui::Painter, text: &str, size: f32, track: f32) -> Arc<Galley> {
    let job = egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat { font_id: deco(size), color: Color32::PLACEHOLDER, extra_letter_spacing: size * track, ..Default::default() },
    );
    painter.layout_job(job)
}

/// A big white name in condensed capitals, shrunk to fit `max_width`.
pub fn title(ui: &mut Ui, text: &str, size: f32, max_width: f32) -> Response {
    let painter = ui.painter().clone();
    let upper = text.to_uppercase();
    let mut galley = painter.layout_no_wrap(upper.clone(), display(size), WHITE);
    if galley.size().x > max_width {
        let fit = (size * max_width / galley.size().x).max(size * 0.5);
        galley = painter.layout_no_wrap(upper, display(fit), WHITE);
    }
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    painter.galley(rect.min, galley, WHITE);
    resp
}

/// `text` on one line, cut with an ellipsis to `max_width`.
pub fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(12.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// The one main action on a screen: a soft pink pill.
    Primary,
    Secondary,
    Ghost,
    /// A navy pill, for the sunset strip.
    Night,
    /// A destructive action offered next to others.
    Danger,
    /// The confirm button of a destructive dialog.
    DangerSolid,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    Small,
    Normal,
    Large,
}

/// Fill, border and text colour of a button.
fn colors(look: Look, t: f32, live: bool) -> (Color32, Stroke, Color32) {
    if !live {
        return match look {
            Look::Ghost | Look::Danger => (Color32::TRANSPARENT, Stroke::NONE, FAINT.gamma_multiply(0.75)),
            _ => (SURFACE, Stroke::new(1.0, LINE), FAINT),
        };
    }
    match look {
        Look::Primary => (mix(PINK, PINK_HI, t), Stroke::NONE, PLUM),
        Look::Secondary => (mix(RAISED, RAISED_HI, t), Stroke::new(1.0, mix(LINE_STRONG, PINK, 0.5 * t)), WHITE),
        Look::Ghost => (mix(Color32::TRANSPARENT, RAISED, t), Stroke::NONE, mix(LILAC, WHITE, t)),
        Look::Night => (mix(BG, NIGHT_HI, t), Stroke::NONE, WHITE),
        Look::Danger => (mix(Color32::TRANSPARENT, BAD.gamma_multiply(0.14), t), Stroke::NONE, BAD),
        Look::DangerSolid => (mix(BAD, WHITE, 0.15 * t), Stroke::NONE, ON_BAD),
    }
}

/// Every button in the launcher: a pill. `busy` shows a spinner in place of
/// the icon and keeps the button's colour while it does not take clicks.
pub fn button_ex(ui: &mut Ui, look: Look, size: Size, icon: Option<char>, label: &str, enabled: bool, busy: bool) -> Response {
    let (h, fs, pad, min_w) = match size {
        Size::Small => (28.0, 13.0, 12.0, 0.0),
        Size::Normal => (CONTROL_H, 14.0, 16.0, 0.0),
        Size::Large => (46.0, 16.0, 26.0, 176.0),
    };
    let painter = ui.painter().clone();
    let label_g = (!label.is_empty()).then(|| painter.layout_no_wrap(label.to_owned(), deco_semi(fs), Color32::PLACEHOLDER));
    let icon_g = icon
        .filter(|_| has_icons() && !busy)
        .map(|c| painter.layout_no_wrap(c.to_string(), glyphs(fs), Color32::PLACEHOLDER));
    let lead = if busy { fs } else { icon_g.as_ref().map_or(0.0, |g| g.size().x) };
    let gap = if lead > 0.0 && label_g.is_some() { 8.0 } else { 0.0 };
    let content = lead + gap + label_g.as_ref().map_or(0.0, |g| g.size().x);
    let w = if label_g.is_none() { h } else { (content + pad * 2.0).max(min_w) };
    let live = enabled && !busy;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), if live { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let t = hover(ui, resp.id, live && resp.hovered());
        let down = live && resp.is_pointer_button_down_on();
        let (fill, stroke, fg) = if busy { colors(look, 0.0, true) } else { colors(look, t, live) };
        // Up 1 px on hover; pressed, 1 px below rest and a touch smaller.
        let mut r = rect.translate(vec2(0.0, if down { 1.0 } else { -t }));
        if down {
            r = r.shrink2(r.size() * 0.0075);
        }
        painter.rect(r, r.height() / 2.0, fill, stroke);
        let mut x = r.center().x - content / 2.0;
        let cy = r.center().y;
        if busy {
            let spin = Rect::from_center_size(pos2(x + lead / 2.0, cy), vec2(lead, lead));
            egui::Spinner::new().size(lead).color(fg).paint_at(ui, spin);
            x += lead + gap;
        } else if let Some(g) = icon_g {
            let gw = g.size().x;
            painter.galley(pos2(x, cy - g.size().y / 2.0), g, fg);
            x += gw + gap;
        }
        if let Some(g) = label_g {
            // Josefin's capitals sit high in its line: a nudge down centres them.
            painter.galley(pos2(x, cy - g.size().y / 2.0 + fs * 0.08), g, fg);
        }
        if resp.has_focus() {
            painter.rect_stroke(r.expand(3.0), r.height() / 2.0 + 3.0, Stroke::new(2.0, PINK));
        }
    }
    if live {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn button(ui: &mut Ui, look: Look, icon: Option<char>, label: &str) -> Response {
    button_ex(ui, look, Size::Normal, icon, label, true, false)
}

pub fn small(ui: &mut Ui, look: Look, icon: Option<char>, label: &str, enabled: bool) -> Response {
    button_ex(ui, look, Size::Small, icon, label, enabled, false)
}

/// A round button with an icon; the words show instead when no icon font was found.
pub fn icon_button(ui: &mut Ui, look: Look, size: Size, icon: char, tip: &str) -> Response {
    let r = if has_icons() {
        button_ex(ui, look, size, Some(icon), "", true, false)
    } else {
        button_ex(ui, look, size, None, tip, true, false)
    };
    r.on_hover_text(tip)
}

/// A one-line field `width` wide, with an optional icon inside on the left.
pub fn field(ui: &mut Ui, value: &mut String, hint: &str, width: f32, lead: Option<char>) -> Response {
    let lead = lead.filter(|_| has_icons());
    let margin = Margin { left: if lead.is_some() { 38.0 } else { 14.0 }, right: 14.0, top: 0.0, bottom: 0.0 };
    let edit = egui::TextEdit::singleline(value)
        .hint_text(RichText::new(hint).color(FAINT))
        .font(regular(14.0))
        .text_color(WHITE)
        .margin(margin)
        .min_size(vec2(width, CONTROL_H))
        .desired_width((width - margin.left - margin.right).max(24.0))
        .vertical_align(egui::Align::Center);
    // A pill, like every button and the filters.
    let pill = CONTROL_H / 2.0;
    let resp = ui
        .scope(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                x.rounding = Rounding::same(pill);
            }
            ui.add(edit)
        })
        .inner;
    // egui 0.29 answers with the text's own box, inside the margin: the field is that plus the margin.
    let outer = resp.rect + margin;
    let focus = ui.ctx().animate_bool_with_time(resp.id.with("focus"), resp.has_focus(), 0.14);
    if focus > 0.0 {
        ui.painter().rect_stroke(outer.expand(2.0), pill + 2.0, Stroke::new(3.0, PINK.gamma_multiply(0.35 * focus)));
    }
    if let Some(c) = lead {
        let color = if resp.has_focus() { PINK } else { FAINT };
        ui.painter().text(pos2(outer.left() + 19.0, outer.center().y), Align2::CENTER_CENTER, c, glyphs(14.0), color);
    }
    resp
}

/// Options side by side in spaced capitals, one picked; its pill slides to
/// a new pick. `width`: the whole control's width (equal parts), else each
/// option's own.
pub fn segmented(ui: &mut Ui, id_salt: &str, options: &[&str], selected: Option<usize>, width: Option<f32>) -> Option<usize> {
    let id = Id::new(id_salt);
    let pad = 3.0;
    let painter = ui.painter().clone();
    let galleys: Vec<Arc<Galley>> = options.iter().map(|o| caps_galley(&painter, o, 11.5, 0.12)).collect();
    let widths: Vec<f32> = match width {
        Some(w) => vec![(w - pad * 2.0) / options.len().max(1) as f32; options.len()],
        None => galleys.iter().map(|g| g.size().x + 32.0).collect(),
    };
    let total = widths.iter().sum::<f32>() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, CONTROL_H), Sense::hover());
    painter.rect(rect, rect.height() / 2.0, FIELD, Stroke::new(1.0, LINE));
    let mut x = rect.left() + pad;
    let cells: Vec<Rect> = widths
        .iter()
        .map(|w| {
            let r = Rect::from_min_size(pos2(x, rect.top() + pad), vec2(*w, rect.height() - pad * 2.0));
            x += w;
            r
        })
        .collect();
    if let Some(cell) = selected.and_then(|i| cells.get(i)) {
        let ctx = ui.ctx();
        let ax = ctx.animate_value_with_time(id.with("x"), cell.left() - rect.left(), 0.24);
        let aw = ctx.animate_value_with_time(id.with("w"), cell.width(), 0.24);
        let pill = Rect::from_min_size(pos2(rect.left() + ax, cell.top()), vec2(aw, cell.height()));
        painter.rect(pill, pill.height() / 2.0, RAISED, Stroke::new(1.0, LINE_HI));
    }
    let mut picked = None;
    for (i, (cell, g)) in cells.iter().zip(galleys).enumerate() {
        let resp = ui.interact(*cell, id.with(i), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let on = selected == Some(i);
        let t = hover(ui, resp.id, resp.hovered());
        let at = cell.center() - g.size() / 2.0 + vec2(0.0, 1.0);
        painter.galley(at, g, if on { WHITE } else { mix(MUTED, WHITE, t) });
        if resp.has_focus() {
            painter.rect_stroke(cell.expand(1.0), cell.height() / 2.0, Stroke::new(2.0, PINK));
        }
        if resp.clicked() && !on {
            picked = Some(i);
        }
    }
    picked
}

/// Spaced-capital tabs on the sunset strip, with a plum line under the open
/// one that slides to the next.
pub fn tabs(ui: &mut Ui, id_salt: &str, options: &[&str], selected: usize) -> Option<usize> {
    let id = Id::new(id_salt);
    let painter = ui.painter().clone();
    let h = ui.available_height();
    let mut cells = Vec::new();
    let mut picked = None;
    for (i, label) in options.iter().enumerate() {
        let g = caps_galley(&painter, label, 12.5, 0.16);
        let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 26.0, h), Sense::click());
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        let t = hover(ui, resp.id, resp.hovered());
        let color = if i == selected { PLUM } else { PLUM.gamma_multiply(0.78 + 0.22 * t) };
        let text_rect = Rect::from_center_size(rect.center() + vec2(0.0, 1.0), g.size());
        painter.galley(text_rect.min, g, color);
        if resp.has_focus() {
            painter.rect_stroke(text_rect.expand2(vec2(9.0, 7.0)), R, Stroke::new(2.0, PLUM));
        }
        if resp.clicked() && i != selected {
            picked = Some(i);
        }
        cells.push(rect);
    }
    if let (Some(first), Some(cell)) = (cells.first(), cells.get(selected)) {
        let ctx = ui.ctx();
        let x = ctx.animate_value_with_time(id.with("x"), cell.left() - first.left(), 0.24);
        let w = ctx.animate_value_with_time(id.with("w"), cell.width(), 0.24);
        let line = Rect::from_min_size(pos2(first.left() + x + 13.0, cell.bottom() - 12.0), vec2(w - 26.0, 2.0));
        painter.rect_filled(line, 1.0, PLUM);
    }
    picked
}

/// A switch with its name and one line on what it does; the whole row toggles.
pub fn toggle(ui: &mut Ui, on: &mut bool, title: &str, hint: &str) -> Response {
    let w = ui.available_width();
    let text_x = 54.0;
    let painter = ui.painter().clone();
    let title_g = painter.layout_no_wrap(title.to_owned(), strong(14.0), WHITE);
    let hint_g = painter.layout(hint.to_owned(), regular(13.0), FAINT, (w - text_x).max(80.0));
    let h = title_g.size().y + 1.0 + hint_g.size().y;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t_on = ui.ctx().animate_bool_with_time(resp.id.with("on"), *on, 0.18);
    let t = hover(ui, resp.id, resp.hovered());
    let track = Rect::from_min_size(pos2(rect.left(), rect.top() + 1.0), vec2(40.0, 22.0));
    let off = mix(RAISED, RAISED_HI, t);
    painter.rect(track, 11.0, mix(off, mix(PINK, PINK_HI, t), t_on), Stroke::new(1.0, mix(LINE_HI, PINK, t_on)));
    let knob = egui::lerp((track.left() + 11.0)..=(track.right() - 11.0), t_on);
    painter.circle_filled(pos2(knob, track.center().y), 8.0, mix(LILAC, PLUM, t_on));
    painter.galley(pos2(rect.left() + text_x, rect.top()), title_g.clone(), WHITE);
    painter.galley(pos2(rect.left() + text_x, rect.top() + title_g.size().y + 1.0), hint_g, FAINT);
    if resp.has_focus() {
        painter.rect_stroke(track.expand(3.0), 14.0, Stroke::new(2.0, PINK));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// A small label in spaced capitals, in a tinted pill.
pub fn badge(ui: &mut Ui, label: &str, color: Color32) -> Response {
    let painter = ui.painter().clone();
    let g = caps_galley(&painter, label, 10.0, 0.12);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 22.0), Sense::hover());
    painter.rect(rect, 11.0, color.gamma_multiply(0.12), Stroke::new(1.0, color.gamma_multiply(0.4)));
    painter.galley(rect.center() - g.size() / 2.0 + vec2(0.0, 1.0), g, color);
    resp
}

/// A status light.
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 3.5, color);
}

/// A short upright line between groups in a row.
pub fn divider(ui: &mut Ui) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, LINE_STRONG));
    ui.add_space(8.0);
}

pub fn icon(ui: &mut Ui, c: char, size: f32, color: Color32) {
    if has_icons() {
        ui.label(RichText::new(c.to_string()).font(glyphs(size)).color(color));
    }
}

/// The one card: a panel lit a little at the top, with a fine border.
pub fn card<R>(ui: &mut Ui, margin: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let back = ui.painter().add(egui::Shape::Noop);
    let resp = Frame::none().stroke(Stroke::new(1.0, LINE)).rounding(R_LG).inner_margin(Margin::same(margin)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
    ui.painter().set(back, panel_shape(resp.response.rect));
    resp.inner
}

/// A card's ground: the panel light, with the card's rounding.
fn panel_shape(rect: Rect) -> egui::epaint::RectShape {
    let mut shape = egui::epaint::RectShape::filled(rect, R_LG, Color32::WHITE);
    match PANEL.get() {
        Some(tex) => {
            shape.fill_texture_id = tex.id();
            shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        }
        None => shape.fill = SURFACE,
    }
    shape
}

/// A tinted note: an icon in `tone`, then whatever `add` puts beside it.
pub fn callout<R>(ui: &mut Ui, tone: Color32, icon_char: char, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::none()
        .fill(tone.gamma_multiply(0.07))
        .stroke(Stroke::new(1.0, tone.gamma_multiply(0.32)))
        .rounding(R)
        .inner_margin(Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if has_icons() {
                    ui.label(RichText::new(icon_char.to_string()).font(glyphs(15.0)).color(tone));
                    ui.add_space(2.0);
                }
                ui.vertical(add).inner
            })
            .inner
        })
        .inner
}

pub struct Stat<'a> {
    pub label: &'a str,
    pub value: String,
    pub color: Color32,
    pub tip: Option<&'a str>,
}

/// Figures side by side on one card, each named above its value.
pub fn stats(ui: &mut Ui, items: &[Stat]) {
    let w = ui.available_width();
    let h = 86.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let painter = ui.painter().clone();
    fill(&painter, rect, R_LG, Fill::Panel, Color32::WHITE);
    painter.rect_stroke(rect, R_LG, Stroke::new(1.0, LINE));
    let cw = w / items.len().max(1) as f32;
    for (i, s) in items.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(rect.left() + cw * i as f32, rect.top()), vec2(cw, h));
        if i > 0 {
            painter.vline(cell.left(), (rect.top() + 18.0)..=(rect.bottom() - 18.0), Stroke::new(1.0, LINE_STRONG));
        }
        let label = caps_galley(&painter, s.label, 10.5, 0.16);
        painter.galley(pos2(cell.left() + 20.0, cell.top() + 17.0), label, LILAC);
        let value = one_line(ui, &s.value.to_uppercase(), display(32.0), s.color, cw - 34.0);
        painter.galley(pos2(cell.left() + 20.0, cell.top() + 36.0), value, s.color);
        if let Some(tip) = s.tip {
            ui.interact(cell, Id::new(("stat", i)), Sense::hover()).on_hover_text(tip);
        }
    }
}

/// The bottom of a scrolled list fading into `ground`: there is more below.
pub fn fade_bottom(painter: &egui::Painter, rect: Rect, ground: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.left_bottom(), ground);
    mesh.colored_vertex(rect.right_bottom(), ground);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

/// The sunset strip across `rect`.
pub fn strip(painter: &egui::Painter, rect: Rect) {
    fill(painter, rect, 0.0, Fill::Strip, Color32::WHITE);
}

/// A dialog over a dimmed window, opened at `opened_at` (it fades in and
/// drops 8 px into place). Returns what `add` returns, and whether the
/// player clicked outside it or pressed Escape.
pub fn modal<R>(ctx: &egui::Context, id: &str, opened_at: f64, width: f32, add: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let now = ctx.input(|i| i.time);
    let a = ease_out(((now - opened_at) / 0.2) as f32);
    if a < 1.0 {
        ctx.request_repaint();
    }
    let screen = ctx.screen_rect();
    let dim = egui::Area::new(Id::new(id).with("dim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(5, 5, 14, (185.0 * a) as u8));
            resp
        })
        .inner;
    let inner = egui::Area::new(Id::new(id))
        .order(egui::Order::Tooltip)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -8.0 * (1.0 - a)))
        .show(ctx, |ui| {
            ui.set_opacity(a);
            // The shadow and the panel go under the contents: their places are kept first.
            let shadow_at = ui.painter().add(egui::Shape::Noop);
            let back = ui.painter().add(egui::Shape::Noop);
            let resp = Frame::none().stroke(Stroke::new(1.0, LINE_STRONG)).rounding(R_LG).inner_margin(Margin::same(28.0)).show(ui, |ui| {
                ui.set_width(width);
                add(ui)
            });
            let rect = resp.response.rect;
            ui.painter().set(shadow_at, shadow().as_shape(rect, R_LG));
            ui.painter().set(back, panel_shape(rect));
            resp.inner
        })
        .inner;
    let dismissed = dim.clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (inner, dismissed)
}
