//! The widgets of the mockup: rounded glass, gradient titles, pills that
//! glow, a gradient card for the selected project and a gradient pill to
//! press. Everything eases when it is touched.

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

pub const ORANGE: Color32 = Color32::from_rgb(255, 122, 60);
pub const CORAL: Color32 = Color32::from_rgb(255, 80, 110);
pub const PINK: Color32 = Color32::from_rgb(240, 60, 170);
pub const PURPLE: Color32 = Color32::from_rgb(130, 60, 230);
pub const GOLD: Color32 = Color32::from_rgb(255, 205, 70);
pub const SKY: Color32 = Color32::from_rgb(70, 170, 255);
pub const RED: Color32 = Color32::from_rgb(255, 80, 90);
pub const GREEN: Color32 = Color32::from_rgb(110, 230, 120);
pub const LIME: Color32 = Color32::from_rgb(150, 220, 90);
pub const TEAL: Color32 = Color32::from_rgb(70, 210, 210);

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// The shapes of a rounded rectangle filled left to right from `a` to `b`:
/// a rounded cap of each colour at the ends and a smooth mesh between.
pub fn gradient_shapes(rect: Rect, rounding: f32, a: Color32, b: Color32) -> Vec<egui::Shape> {
    let r = rounding.min(rect.height() / 2.0).min(rect.width() / 2.0);
    // The caps reach to where the mesh starts: nothing is painted twice.
    let left = Rect::from_min_max(rect.min, Pos2::new(rect.left() + r, rect.bottom()));
    let right = Rect::from_min_max(Pos2::new(rect.right() - r, rect.top()), rect.max);
    let mut mesh = egui::Mesh::default();
    let (x0, x1) = (rect.left() + r, rect.right() - r);
    mesh.colored_vertex(Pos2::new(x0, rect.top()), a);
    mesh.colored_vertex(Pos2::new(x1, rect.top()), b);
    mesh.colored_vertex(Pos2::new(x0, rect.bottom()), a);
    mesh.colored_vertex(Pos2::new(x1, rect.bottom()), b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    let lr = Rounding {
        nw: r,
        sw: r,
        ne: 0.0,
        se: 0.0,
    };
    let rr = Rounding {
        nw: 0.0,
        sw: 0.0,
        ne: r,
        se: r,
    };
    vec![
        egui::Shape::rect_filled(left, lr, a),
        egui::Shape::rect_filled(right, rr, b),
        egui::Shape::mesh(mesh),
    ]
}

pub fn gradient_rect(painter: &egui::Painter, rect: Rect, rounding: f32, a: Color32, b: Color32) {
    painter.add(egui::Shape::Vec(gradient_shapes(rect, rounding, a, b)));
}

/// A soft glow around a pill: layers that grow outward, each a whole pill
/// (its rounding is half its own height), fading as they grow.
pub fn pill_glow(painter: &egui::Painter, rect: Rect, color: Color32, reach: f32, strength: f32) {
    let painter = painter.clone().with_clip_rect(painter.ctx().screen_rect());
    let layers = 8;
    for k in (1..=layers).rev() {
        let g = reach * k as f32 / layers as f32;
        let e = rect.expand(g);
        let fade = 1.0 - k as f32 / (layers as f32 + 1.0);
        painter.rect_filled(
            e,
            e.height() / 2.0,
            alpha(color, strength * fade * fade / layers as f32 * 2.5),
        );
    }
}

/// See-through dark glass with rounded corners and a faint light edge.
pub fn glass() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(18, 10, 28, 165))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26),
        ))
        .rounding(Rounding::same(0.0))
        .inner_margin(16.0)
}

/// Text whose letters run from `a` to `b`, with a soft glow and a thin dark
/// outline so it reads over any picture.
pub fn gradient_title(
    ui: &mut egui::Ui,
    text: &str,
    size: f32,
    a: Color32,
    b: Color32,
) -> egui::Response {
    let font = FontId::proportional(size);
    let n = text.chars().count().max(2);
    let mut job = egui::text::LayoutJob::default();
    for (i, ch) in text.chars().enumerate() {
        let c = mix(a, b, i as f32 / (n - 1) as f32);
        job.append(
            &ch.to_string(),
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: c,
                ..Default::default()
            },
        );
    }
    let galley = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let p = ui.painter();
    // Glow, then outline, then the letters.
    for (dx, dy, al) in [
        (-3.0, 0.0, 0.16),
        (3.0, 0.0, 0.16),
        (0.0, -3.0, 0.12),
        (0.0, 4.0, 0.2),
    ] {
        let g =
            ui.painter()
                .layout_no_wrap(text.to_string(), font.clone(), alpha(mix(a, b, 0.5), al));
        p.galley(rect.min + Vec2::new(dx, dy), g, alpha(a, al));
    }
    let shadow = ui.painter().layout_no_wrap(
        text.to_string(),
        font.clone(),
        Color32::from_black_alpha(150),
    );
    p.galley(rect.min + Vec2::new(2.0, 3.0), shadow, Color32::BLACK);
    p.galley(rect.min, galley, Color32::WHITE);
    resp
}

/// A filter pill: gradient when on, outlined when off.
pub fn filter_pill(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let font = FontId::proportional(16.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let size = galley.size() + Vec2::new(32.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.15);
    let p = ui.painter();
    let r = rect.height() / 2.0;
    if on {
        gradient_rect(p, rect, r, ORANGE, CORAL);
    } else {
        p.rect_filled(
            rect,
            r,
            Color32::from_rgba_unmultiplied(20, 14, 30, (150.0 + 60.0 * h) as u8),
        );
        p.rect_stroke(
            rect,
            r,
            Stroke::new(1.0_f32, alpha(Color32::WHITE, 0.35 + 0.35 * h)),
        );
    }
    p.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    resp
}

/// A filter pill that opens a menu: as [`filter_pill`], with a drawn down-arrow
/// (the font has no arrow glyph).
pub fn dropdown_pill(ui: &mut egui::Ui, text: &str, open: bool) -> egui::Response {
    let font = FontId::proportional(16.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let size = galley.size() + Vec2::new(32.0 + 18.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered() || open, 0.15);
    let p = ui.painter();
    let r = rect.height() / 2.0;
    p.rect_filled(
        rect,
        r,
        Color32::from_rgba_unmultiplied(20, 14, 30, (150.0 + 60.0 * h) as u8),
    );
    p.rect_stroke(
        rect,
        r,
        Stroke::new(1.0_f32, alpha(Color32::WHITE, 0.35 + 0.35 * h)),
    );
    let text_at = Pos2::new(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0);
    p.galley(text_at, galley, Color32::WHITE);
    let c = Pos2::new(rect.right() - 20.0, rect.center().y);
    let (w, t) = (5.0, 3.0);
    let pts = if open {
        vec![
            Pos2::new(c.x - w, c.y + t),
            Pos2::new(c.x + w, c.y + t),
            Pos2::new(c.x, c.y - t),
        ]
    } else {
        vec![
            Pos2::new(c.x - w, c.y - t),
            Pos2::new(c.x + w, c.y - t),
            Pos2::new(c.x, c.y + t),
        ]
    };
    p.add(egui::Shape::convex_polygon(
        pts,
        alpha(Color32::WHITE, 0.75 + 0.25 * h),
        Stroke::NONE,
    ));
    resp
}

/// A tag: a glowing pill with coloured text.
pub fn tag(ui: &mut egui::Ui, text: &str, color: Color32, big: bool) {
    let font = FontId::proportional(if big { 18.0 } else { 13.0 });
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), font, mix(color, Color32::WHITE, 0.15));
    let pad = if big {
        Vec2::new(30.0, 16.0)
    } else {
        Vec2::new(14.0, 6.0)
    };
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad, Sense::hover());
    let p = ui.painter();
    let r = if big { rect.height() / 2.0 } else { 3.0 };
    if big {
        pill_glow(p, rect, color, 8.0, 0.45);
    }
    let body = mix(Color32::from_rgb(18, 10, 26), color, 0.22);
    p.rect_filled(
        rect,
        r,
        Color32::from_rgba_unmultiplied(body.r(), body.g(), body.b(), 225),
    );
    p.rect_stroke(
        rect,
        r,
        Stroke::new(if big { 2.0_f32 } else { 1.0_f32 }, color),
    );
    p.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// A text link with an underline that brightens on hover.
pub fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let font = FontId::proportional(17.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(0.0, 4.0), Sense::click());
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.15);
    let c = mix(Color32::from_rgb(220, 215, 230), Color32::WHITE, h);
    let p = ui.painter();
    p.galley(rect.min, galley, c);
    p.line_segment(
        [
            Pos2::new(rect.left(), rect.bottom() - 1.0),
            Pos2::new(rect.right(), rect.bottom() - 1.0),
        ],
        Stroke::new(1.0_f32, alpha(c, 0.6 + 0.4 * h)),
    );
    resp
}

/// A project card: rounded, glass with a coloured edge, lifting on hover;
/// the selected one in an orange-to-purple gradient.
pub fn card(
    ui: &mut egui::Ui,
    id: egui::Id,
    selected: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let hover_id = id.with("hover");
    let hovered_before = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(hover_id))
        .unwrap_or(false);
    let h = ui
        .ctx()
        .animate_bool_with_time(hover_id.with("anim"), hovered_before, 0.18);
    let s = ui
        .ctx()
        .animate_bool_with_time(id.with("sel"), selected, 0.25);
    let width = ui.available_width();
    let bg_idx = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::none()
        .inner_margin(egui::Margin {
            left: 18.0,
            right: 14.0,
            top: 12.0,
            bottom: 12.0,
        })
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            add(ui);
        });
    let rect = inner.response.rect;
    let p = ui.painter().clone();
    let r = 10.0;
    // Background drawn under the content: a shadow, the body, the edge.
    let mut shapes = vec![egui::Shape::rect_filled(
        rect.translate(Vec2::new(0.0, 4.0 + 3.0 * h)),
        r,
        Color32::from_black_alpha((60.0 + 60.0 * h) as u8),
    )];
    let body = mix(
        Color32::from_rgba_unmultiplied(16, 10, 26, 205),
        Color32::from_rgba_unmultiplied(30, 18, 44, 220),
        h,
    );
    shapes.push(egui::Shape::rect_filled(rect, r, body));
    p.set(bg_idx, egui::Shape::Vec(shapes));
    if s > 0.01 {
        // The gradient fades in under the card's text.
        let a = alpha(mix(ORANGE, CORAL, 0.2), 0.9 * s);
        let b = alpha(PURPLE, 0.85 * s);
        gradient_under(ui, bg_idx, rect, r, a, b);
    }
    let edge = if selected {
        alpha(mix(ORANGE, PINK, 0.5), 0.9)
    } else {
        mix(alpha(PURPLE, 0.55), alpha(PINK, 0.9), h)
    };
    p.rect_stroke(
        rect,
        r,
        Stroke::new(if selected { 2.0_f32 } else { 1.2_f32 }, edge),
    );
    let resp = ui.interact(rect, id, Sense::click());
    ui.ctx()
        .data_mut(|d| d.insert_temp(hover_id, resp.hovered()));
    resp
}

/// The selected card's gradient, put into the background slot `idx` with
/// the body so the card's text stays on top.
fn gradient_under(
    ui: &egui::Ui,
    idx: egui::layers::ShapeIdx,
    rect: Rect,
    r: f32,
    a: Color32,
    b: Color32,
) {
    let mut shapes = vec![
        egui::Shape::rect_filled(
            rect.translate(Vec2::new(0.0, 6.0)),
            r,
            Color32::from_black_alpha(90),
        ),
        egui::Shape::rect_filled(rect, r, Color32::from_rgba_unmultiplied(16, 10, 26, 205)),
    ];
    shapes.extend(gradient_shapes(rect, r, a, b));
    ui.painter().set(idx, egui::Shape::Vec(shapes));
}

/// The big pill to press: an orange-to-purple gradient that glows and lifts
/// on hover; grey when it cannot be pressed.
pub fn big_pill(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    enabled: bool,
    busy: bool,
    t: f32,
) -> egui::Response {
    let size = Vec2::new(300.0, 58.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("h"), enabled && resp.hovered(), 0.15);
    let rect = rect.shrink(0.0);
    let p = ui.painter();
    let r = rect.height() / 2.0;
    if enabled {
        gradient_rect(
            p,
            rect,
            r,
            mix(ORANGE, GOLD, 0.15 * h),
            mix(PURPLE, PINK, 0.2 + 0.2 * h),
        );
    } else {
        p.rect_filled(rect, r, Color32::from_rgba_unmultiplied(50, 40, 64, 220));
    }
    let text = if busy {
        format!("{label}{}", ".".repeat(((t * 3.0) as usize % 3) + 1))
    } else {
        label.to_string()
    };
    let color = if enabled {
        Color32::WHITE
    } else {
        alpha(Color32::WHITE, 0.4)
    };
    let font = FontId::proportional(26.0);
    let g_icon = ui
        .painter()
        .layout_no_wrap(icon.to_string(), font.clone(), color);
    let g_text = ui.painter().layout_no_wrap(text, font, color);
    let total = g_icon.size().x + 12.0 + g_text.size().x;
    let x = rect.center().x - total / 2.0;
    let y = rect.center().y - g_text.size().y / 2.0;
    p.galley(Pos2::new(x, y), g_icon.clone(), color);
    p.galley(Pos2::new(x + g_icon.size().x + 12.0, y), g_text, color);
    resp
}
