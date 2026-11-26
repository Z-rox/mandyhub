//! The look: a neon sunset over a moving grid, sharp corners, glowing text,
//! and widgets that move when they are touched.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};

pub const PINK: Color32 = Color32::from_rgb(255, 46, 150);
pub const ORANGE: Color32 = Color32::from_rgb(255, 150, 50);
pub const PURPLE: Color32 = Color32::from_rgb(120, 50, 255);

/// Seconds since the launcher started.
pub fn time(ctx: &egui::Context) -> f32 {
    ctx.input(|i| i.time) as f32
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// A colour that drifts between pink, orange and purple.
pub fn shimmer(t: f32, phase: f32) -> Color32 {
    let x = (t * 0.6 + phase).sin() * 0.5 + 0.5;
    let y = (t * 0.37 + phase * 1.7).sin() * 0.5 + 0.5;
    mix(mix(PINK, ORANGE, x), PURPLE, y * 0.35)
}

/// A vertical gradient across `rect`, `stops` from top to bottom.
fn gradient(painter: &egui::Painter, rect: Rect, stops: &[(f32, Color32)]) {
    let mut mesh = egui::Mesh::default();
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        let y0 = rect.top() + rect.height() * t0;
        let y1 = rect.top() + rect.height() * t1;
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(Pos2::new(rect.left(), y0), c0);
        mesh.colored_vertex(Pos2::new(rect.right(), y0), c0);
        mesh.colored_vertex(Pos2::new(rect.left(), y1), c1);
        mesh.colored_vertex(Pos2::new(rect.right(), y1), c1);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 3, i + 2);
    }
    painter.add(Shape::mesh(mesh));
}

/// The background of the whole window: sky, a striped sun, a grid running
/// toward the viewer, and drifting stars. Asks for the next frame.
/// The time the background is drawn at: the clock while it moves, the
/// moment it stopped at while it stands still (a game is running).
pub fn anim_time(ctx: &egui::Context, animate: bool) -> f32 {
    static FROZEN: std::sync::Mutex<f32> = std::sync::Mutex::new(0.0);
    if animate {
        let t = time(ctx);
        *FROZEN.lock().unwrap() = t;
        t
    } else {
        *FROZEN.lock().unwrap()
    }
}

pub fn background(ctx: &egui::Context, animate: bool) {
    let t = anim_time(ctx, animate);
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let horizon = rect.top() + rect.height() * 0.62;

    // Sky.
    gradient(
        &painter,
        Rect::from_min_max(rect.min, Pos2::new(rect.right(), horizon)),
        &[
            (0.0, Color32::from_rgb(8, 4, 24)),
            (0.55, Color32::from_rgb(40, 10, 70)),
            (0.85, Color32::from_rgb(140, 30, 110)),
            (1.0, Color32::from_rgb(255, 110, 90)),
        ],
    );
    // Ground.
    gradient(
        &painter,
        Rect::from_min_max(Pos2::new(rect.left(), horizon), rect.max),
        &[
            (0.0, Color32::from_rgb(30, 6, 50)),
            (1.0, Color32::from_rgb(4, 2, 12)),
        ],
    );

    // Stars, twinkling.
    for i in 0..70 {
        let fx = ((i as f32 * 127.1).sin() * 43758.5).fract().abs();
        let fy = ((i as f32 * 311.7).sin() * 12345.6).fract().abs();
        let x = rect.left() + fx * rect.width();
        let y = rect.top() + fy * (horizon - rect.top()) * 0.7;
        let tw = ((t * (1.0 + fx * 2.0) + i as f32).sin() * 0.5 + 0.5) * 0.8;
        painter.circle_filled(Pos2::new(x, y), 1.0 + fy, alpha(Color32::WHITE, tw * 0.7));
    }

    // The sun: stripes cut through its lower half, sliding down slowly.
    let center = Pos2::new(rect.center().x + rect.width() * 0.18, horizon - 10.0);
    let r = rect.height() * 0.22;
    for k in 0..3 {
        let glow = r + 18.0 + k as f32 * 22.0;
        painter.circle_filled(center, glow, alpha(PINK, 0.05));
    }
    let rows = 60;
    for row in 0..rows {
        let y0 = center.y - r + (row as f32 / rows as f32) * 2.0 * r;
        let y1 = y0 + 2.0 * r / rows as f32 + 0.5;
        if y0 > center.y + 2.0 {
            break;
        }
        // Gaps that move downward in the lower half.
        let below = (y0 - (center.y - r * 0.35)) / (r * 0.4);
        if below > 0.0 {
            let band = ((y0 * 0.18) - t * 2.2).sin();
            if band > 0.55 - below * 0.6 {
                continue;
            }
        }
        let dy = (center.y - (y0 + y1) * 0.5).abs();
        let half = (r * r - dy * dy).max(0.0).sqrt();
        let c = mix(
            Color32::from_rgb(255, 220, 90),
            PINK,
            (row as f32 / rows as f32) * 1.6,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(center.x - half, y0),
                Pos2::new(center.x + half, y1),
            ),
            0.0,
            c,
        );
    }

    // The grid: lines toward a vanishing point, and rows running at you.
    let vp = Pos2::new(rect.center().x, horizon);
    let grid = alpha(PINK, 0.55);
    for i in -18..=18 {
        let x = rect.center().x + i as f32 * rect.width() / 9.0;
        painter.line_segment(
            [vp, Pos2::new(x, rect.bottom())],
            Stroke::new(1.2_f32, grid),
        );
    }
    let speed = (t * 0.35).fract();
    for i in 0..14 {
        let z = (i as f32 + speed) / 14.0;
        let y = horizon + (rect.bottom() - horizon) * z * z;
        let a = 0.15 + z * 0.65;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0 + z, alpha(PINK, a)),
        );
    }
    painter.line_segment(
        [
            Pos2::new(rect.left(), horizon),
            Pos2::new(rect.right(), horizon),
        ],
        Stroke::new(2.0_f32, alpha(ORANGE, 0.9)),
    );

    // Scanlines over everything.
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, Color32::from_black_alpha(28)),
        );
        y += 3.0;
    }

    if animate {
        // About 30 frames a second: smooth, and light on the card.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// Larger text sizes for the whole launcher.
pub fn text_sizes(ctx: &egui::Context) {
    use egui::{FontId, TextStyle};
    let mut s = (*ctx.style()).clone();
    s.text_styles
        .insert(TextStyle::Body, FontId::proportional(15.5));
    s.text_styles
        .insert(TextStyle::Button, FontId::proportional(15.5));
    s.text_styles
        .insert(TextStyle::Small, FontId::proportional(12.5));
    s.text_styles
        .insert(TextStyle::Heading, FontId::proportional(24.0));
    ctx.set_style(s);
}

/// Text with a glow behind it.
pub fn glow_text(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) -> egui::Response {
    let font = egui::FontId::proportional(size);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let painter = ui.painter();
    for (dx, dy, a) in [
        (-2.0, 0.0, 0.25),
        (2.0, 0.0, 0.25),
        (0.0, -2.0, 0.25),
        (0.0, 2.0, 0.25),
        (-4.0, 0.0, 0.1),
        (4.0, 0.0, 0.1),
    ] {
        let g = ui.painter().layout_no_wrap(
            text.to_string(),
            egui::FontId::proportional(size),
            alpha(color, a),
        );
        painter.galley(rect.min + Vec2::new(dx, dy), g, alpha(color, a));
    }
    painter.galley(rect.min, galley, color);
    resp
}

/// A glass panel: dark, see-through, a neon line on top.
pub fn glass() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(10, 6, 24, 200))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(255, 46, 150, 90),
        ))
        .inner_margin(14.0)
}

/// A clickable card that lights up, slides and grows a neon bar on hover;
/// a selected one has a pulsing border. Returns the click response.
pub fn card(
    ui: &mut egui::Ui,
    id: egui::Id,
    selected: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let t = time(ui.ctx());
    let hover_id = id.with("hover");
    let hovered_before = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(hover_id))
        .unwrap_or(false);
    let h =
        ui.ctx()
            .animate_bool_with_time(hover_id.with("anim"), hovered_before || selected, 0.18);
    let slide = 6.0 * h;
    let width = ui.available_width();
    let inner = egui::Frame::none()
        .fill(mix(
            Color32::from_rgba_unmultiplied(20, 12, 44, 215),
            Color32::from_rgba_unmultiplied(70, 20, 90, 230),
            h,
        ))
        .inner_margin(egui::Margin {
            left: 16.0 + slide,
            right: 12.0,
            top: 10.0,
            bottom: 10.0,
        })
        .show(ui, |ui| {
            ui.set_width(width - 28.0 - slide);
            add(ui);
        });
    let rect = inner.response.rect;
    let painter = ui.painter();
    // The neon bar on the left grows with the hover.
    let bar = Rect::from_min_size(rect.min, Vec2::new(3.0 + 5.0 * h, rect.height()));
    painter.rect_filled(
        bar,
        0.0,
        if selected {
            shimmer(t, 0.0)
        } else {
            mix(PURPLE, PINK, h)
        },
    );
    if selected {
        let pulse = (t * 3.0).sin() * 0.5 + 0.5;
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.5 + pulse, alpha(PINK, 0.6 + 0.4 * pulse)),
        );
    } else if h > 0.01 {
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, alpha(PINK, 0.5 * h)));
    }
    let resp = ui.interact(rect, id, Sense::click());
    ui.ctx()
        .data_mut(|d| d.insert_temp(hover_id, resp.hovered()));
    resp
}

/// The big button: a moving light sweep across a neon gradient and a
/// pulsing glow while it can be pressed.
pub fn big_button(ui: &mut egui::Ui, label: &str, enabled: bool, busy: bool) -> egui::Response {
    let t = time(ui.ctx());
    let size = Vec2::new(260.0, 56.0);
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
    let painter = ui.painter();
    if enabled {
        let pulse = (t * 2.4).sin() * 0.5 + 0.5;
        for k in 1..=4 {
            let grow = k as f32 * (3.0 + 3.0 * pulse + 2.0 * h);
            painter.rect_filled(rect.expand(grow), 0.0, alpha(PINK, 0.07 - k as f32 * 0.012));
        }
        // Gradient body.
        let mut mesh = egui::Mesh::default();
        let (a, b) = (mix(PINK, ORANGE, h * 0.4), mix(PURPLE, PINK, 0.3 + h * 0.3));
        mesh.colored_vertex(rect.left_top(), a);
        mesh.colored_vertex(rect.right_top(), b);
        mesh.colored_vertex(rect.left_bottom(), a);
        mesh.colored_vertex(rect.right_bottom(), b);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        painter.add(Shape::mesh(mesh));
        // The sweep: a bright slanted band moving across.
        let x = rect.left() + ((t * 0.5).fract() * 1.6 - 0.3) * rect.width();
        let band = vec![
            Pos2::new(x, rect.top()),
            Pos2::new(x + 30.0, rect.top()),
            Pos2::new(x + 10.0, rect.bottom()),
            Pos2::new(x - 20.0, rect.bottom()),
        ];
        let clip = painter.clip_rect().intersect(rect);
        painter.with_clip_rect(clip).add(Shape::convex_polygon(
            band,
            alpha(Color32::WHITE, 0.22),
            Stroke::NONE,
        ));
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(2.0_f32, alpha(Color32::WHITE, 0.5 + 0.5 * h)),
        );
    } else {
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(40, 30, 60, 220));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, alpha(Color32::WHITE, 0.15)));
    }
    let text = if busy {
        let dots = ".".repeat(((t * 3.0) as usize % 3) + 1);
        format!("{label}{dots}")
    } else {
        label.to_string()
    };
    painter.text(
        rect.center()
            + Vec2::new(
                0.0,
                if resp.is_pointer_button_down_on() {
                    1.0
                } else {
                    0.0
                },
            ),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(24.0),
        if enabled {
            Color32::WHITE
        } else {
            alpha(Color32::WHITE, 0.35)
        },
    );
    resp
}

/// A neon tag, sharp.
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(alpha(color, 0.14))
        .stroke(Stroke::new(1.0_f32, color))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(color).size(12.0).strong());
        });
}
