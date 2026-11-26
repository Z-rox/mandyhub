//! The player's own pictures across the top of a project's page: each slowly
//! drifts closer and fades into the next, and their lower part fades into the
//! page's ground. They come from `%LOCALAPPDATA%\Mandyhub\backgrounds` (jpg or
//! png); none ship with the launcher, and without any the page has no banner.

use eframe::egui::{self, Color32, Pos2, Rect};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Seconds each picture is shown, and of the fade between two.
const HOLD: f32 = 9.0;
const FADE: f32 = 2.0;

pub fn folder() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Mandyhub").join("backgrounds")
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
            let mut files: Vec<PathBuf> = std::fs::read_dir(folder())
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            files.retain(|f| {
                let e = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                e == "jpg" || e == "jpeg" || e == "png"
            });
            files.sort();
            for f in files.into_iter().take(24) {
                let Ok(img) = image::open(&f) else { continue };
                // At most 1920 wide: enough for the window, light on memory.
                let img = if img.width() > 1920 { img.resize(1920, 1920, image::imageops::FilterType::Triangle) } else { img };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                out.lock().unwrap().push(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()));
            }
        });
        Slides { decoded, textures: Vec::new() }
    }

    /// Takes in the pictures read so far; true when there is one to show.
    pub fn ready(&mut self, ctx: &egui::Context) -> bool {
        for img in self.decoded.lock().unwrap().drain(..) {
            let name = format!("slide-{}", self.textures.len());
            self.textures.push(ctx.load_texture(name, img, egui::TextureOptions::LINEAR));
        }
        !self.textures.is_empty()
    }

    /// Paints the pictures into `rect` at time `t`, the lower part fading into `ground`.
    pub fn paint_in(&self, painter: &egui::Painter, rect: Rect, t: f32, ground: Color32) {
        if self.textures.is_empty() {
            return;
        }
        painter.rect_filled(rect, 0.0, ground);
        let n = self.textures.len();
        let k = (t / HOLD).floor() as usize;
        let local = t - k as f32 * HOLD;
        // The picture now, and the one before it while this one fades in.
        let draw = |idx: usize, since: f32, alpha: f32| {
            let tex = &self.textures[idx % n];
            let [w, h] = tex.size();
            let (img_aspect, box_aspect) = (w as f32 / h as f32, rect.width() / rect.height());
            // Cover the box: crop the picture to its shape.
            let (mut uw, mut uh) = if img_aspect > box_aspect { (box_aspect / img_aspect, 1.0) } else { (1.0, img_aspect / box_aspect) };
            // Slowly closer: 100% to 94% of the crop over its time.
            let p = (since / (HOLD + FADE)).clamp(0.0, 1.0);
            let zoom = 1.0 - 0.06 * p;
            uw *= zoom;
            uh *= zoom;
            let dir = match idx % 4 {
                0 => (1.0, 0.4),
                1 => (-1.0, 0.3),
                2 => (0.6, -1.0),
                _ => (-0.5, -0.8),
            };
            let (cx, cy) = (0.5 + dir.0 * (1.0 - uw) * 0.5 * (p - 0.5), 0.5 + dir.1 * (1.0 - uh) * 0.5 * (p - 0.5));
            let uv = Rect::from_center_size(Pos2::new(cx, cy), egui::vec2(uw, uh));
            painter.image(tex.id(), rect, uv, Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8));
        };
        if local < FADE && k > 0 {
            draw(k - 1, local + HOLD, 1.0);
            draw(k, local, local / FADE);
        } else {
            draw(k, local, 1.0);
        }
        // A light veil over the top, and the ground rising through the lower half.
        let at = |a: u8| Color32::from_rgba_unmultiplied(ground.r(), ground.g(), ground.b(), a);
        let stops = [(0.0, at(70)), (0.45, at(110)), (1.0, at(255))];
        let mut mesh = egui::Mesh::default();
        for w in stops.windows(2) {
            let (y0, c0) = (rect.top() + rect.height() * w[0].0, w[0].1);
            let (y1, c1) = (rect.top() + rect.height() * w[1].0, w[1].1);
            let i = mesh.vertices.len() as u32;
            mesh.colored_vertex(Pos2::new(rect.left(), y0), c0);
            mesh.colored_vertex(Pos2::new(rect.right(), y0), c0);
            mesh.colored_vertex(Pos2::new(rect.left(), y1), c1);
            mesh.colored_vertex(Pos2::new(rect.right(), y1), c1);
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}
