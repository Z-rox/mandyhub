//! The background like a loading screen: big pictures that slowly zoom and
//! drift, each fading into the next. The pictures are the player's own, from
//! `%LOCALAPPDATA%\Mandyhub\backgrounds` (jpg or png); none ship with the
//! launcher.

use eframe::egui::{self, Color32, Pos2, Rect};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Seconds each picture is shown, and of the fade between two.
const HOLD: f32 = 9.0;
const FADE: f32 = 2.0;

pub fn folder() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Mandyhub").join("backgrounds")
}

/// The pictures in [`folder`] (jpg, jpeg, png), sorted by name.
pub fn list() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder())
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|f| {
        let e = f
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        e == "jpg" || e == "jpeg" || e == "png"
    });
    files.sort();
    files
}

/// Copies `from` into [`folder`] under a name not taken yet; the copy's path.
pub fn add(from: &std::path::Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder())?;
    let stem = from
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("picture");
    let ext = from
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();
    let mut to = folder().join(format!("{stem}.{ext}"));
    let mut n = 2;
    while to.exists() {
        to = folder().join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    std::fs::copy(from, &to)?;
    Ok(to)
}

pub struct Slides {
    decoded: Arc<Mutex<Vec<egui::ColorImage>>>,
    textures: Vec<egui::TextureHandle>,
}

impl Slides {
    /// Starts reading the pictures on a thread of its own.
    pub fn new() -> Slides {
        let decoded = Arc::new(Mutex::new(Vec::new()));
        let out = decoded.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(folder());
            for f in list().into_iter().take(24) {
                let Ok(img) = image::open(&f) else { continue };
                // At most 1920 wide: enough for the window, light on memory.
                let img = if img.width() > 1920 {
                    img.resize(1920, 1920, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                out.lock()
                    .unwrap()
                    .push(egui::ColorImage::from_rgba_unmultiplied(
                        size,
                        rgba.as_raw(),
                    ));
            }
        });
        Slides {
            decoded,
            textures: Vec::new(),
        }
    }

    /// Paints the pictures over the whole window at time `t`; false if there
    /// are none (then the caller paints something else).
    pub fn paint(&mut self, ctx: &egui::Context, t: f32) -> bool {
        for (i, img) in self.decoded.lock().unwrap().drain(..).enumerate() {
            let name = format!("slide-{}-{i}", self.textures.len());
            self.textures
                .push(ctx.load_texture(name, img, egui::TextureOptions::LINEAR));
        }
        if self.textures.is_empty() {
            return false;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.rect_filled(screen, 0.0, Color32::BLACK);
        let n = self.textures.len();
        let step = HOLD;
        let k = (t / step).floor() as usize;
        let local = t - k as f32 * step;
        // The picture now, and the one before it while fading in.
        let draw = |idx: usize, since: f32, alpha: f32| {
            let tex = &self.textures[idx % n];
            let [w, h] = tex.size();
            let (img_aspect, scr_aspect) = (w as f32 / h as f32, screen.width() / screen.height());
            // Cover the window: crop the picture to the window's shape.
            let (mut uw, mut uh) = if img_aspect > scr_aspect {
                (scr_aspect / img_aspect, 1.0)
            } else {
                (1.0, img_aspect / scr_aspect)
            };
            // Slowly closer: 100% to 88% of the picture over its time.
            let p = (since / (HOLD + FADE)).clamp(0.0, 1.0);
            let zoom = 1.0 - 0.12 * p;
            uw *= zoom;
            uh *= zoom;
            // Drift: each picture toward its own corner.
            let dir = match idx % 4 {
                0 => (1.0, 0.4),
                1 => (-1.0, 0.3),
                2 => (0.6, -1.0),
                _ => (-0.5, -0.8),
            };
            let (cx, cy) = (
                0.5 + dir.0 * (1.0 - uw) * 0.5 * (p - 0.5),
                0.5 + dir.1 * (1.0 - uh) * 0.5 * (p - 0.5),
            );
            let uv = Rect::from_center_size(Pos2::new(cx, cy), egui::vec2(uw, uh));
            let tint = Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8);
            painter.image(tex.id(), screen, uv, tint);
        };
        if local < FADE && k > 0 {
            draw(k - 1, local + HOLD, 1.0);
            draw(k, local, local / FADE);
        } else {
            draw(k, local, 1.0);
        }
        // Darker toward the top and bottom, a sunset tint over all: the
        // panels stay readable over any picture.
        let mut mesh = egui::Mesh::default();
        let stops = [
            (0.0, Color32::from_rgba_unmultiplied(10, 4, 24, 200)),
            (0.35, Color32::from_rgba_unmultiplied(30, 8, 50, 90)),
            (0.7, Color32::from_rgba_unmultiplied(60, 10, 60, 110)),
            (1.0, Color32::from_rgba_unmultiplied(8, 2, 18, 230)),
        ];
        for w in stops.windows(2) {
            let (y0, c0) = (screen.top() + screen.height() * w[0].0, w[0].1);
            let (y1, c1) = (screen.top() + screen.height() * w[1].0, w[1].1);
            let i = mesh.vertices.len() as u32;
            mesh.colored_vertex(Pos2::new(screen.left(), y0), c0);
            mesh.colored_vertex(Pos2::new(screen.right(), y0), c0);
            mesh.colored_vertex(Pos2::new(screen.left(), y1), c1);
            mesh.colored_vertex(Pos2::new(screen.right(), y1), c1);
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
        true
    }
}
