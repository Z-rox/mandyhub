//! What Mandyhub shows: the sunset strip with its tabs, the project list, a
//! project's page, the settings page, the status bar and the dialogs. The
//! work they start (install, play, update checks, moving folders) lives in
//! main.rs.

use crate::theme::{self, glyph, Look, Size, Stat};
use crate::{built_exe, default_workspace, parse_repo, search_drives, slides, tools, workspace};
use crate::{AddRepo, App, Kind, Project, RepoState, View};
use eframe::egui::{self, vec2, Align, Color32, Frame, Label, Layout, Margin, RichText, Sense, Stroke, Ui};
use std::time::Duration;

/// The graphics modes: name, Series S profile, render scale, what it does.
const MODES: [(&str, bool, f32, &str); 3] = [
    ("Fast", true, 0.5, "Renders at half resolution and scales up."),
    ("Balanced", true, 0.75, "Renders at three quarters and scales up."),
    ("Best quality", false, 1.0, "Renders at full resolution."),
];

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::regular(13.0)).color(theme::FAINT));
}

fn wrapped(ui: &mut Ui, text: &str, color: Color32) {
    ui.add(Label::new(RichText::new(text).font(theme::regular(14.0)).color(color)).wrap());
}

/// A list heading, in line with the text of the rows under it.
fn group(ui: &mut Ui, text: &str, count: Option<usize>) {
    // More air above than below: it heads the rows, not the controls over it.
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        theme::caps(ui, text, 10.5, theme::LILAC);
        if let Some(n) = count {
            theme::caps(ui, &n.to_string(), 10.5, theme::FAINT);
        }
    });
}

/// A settings section: a card with a name, a line on what it is for, and its controls.
fn section(ui: &mut Ui, name: &str, badge: Option<&str>, intro: &str, add: impl FnOnce(&mut Ui)) {
    theme::card(ui, 24.0, |ui| {
        ui.horizontal(|ui| {
            theme::caps(ui, name, 13.0, theme::WHITE);
            if let Some(b) = badge {
                theme::badge(ui, b, theme::WARN);
            }
        });
        if !intro.is_empty() {
            ui.add_space(2.0);
            wrapped(ui, intro, theme::FAINT);
        }
        ui.add_space(14.0);
        add(ui);
    });
}

/// Right-aligned buttons on a row of their own.
fn button_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 46.0), Layout::right_to_left(Align::Center), add);
}

fn gpu_text(gpu: &[String]) -> String {
    gpu.join(", ")
}

/// A running job's stage in a word or two, for its button.
fn short_stage(stage: &str) -> &'static str {
    if stage.starts_with("Downloading") {
        "Downloading"
    } else if stage.starts_with("Building") {
        "Building"
    } else if stage == "Playing" {
        "Playing"
    } else if stage.starts_with("Uninstall") {
        "Uninstalling"
    } else if stage.starts_with("Warming") {
        "Warming up"
    } else {
        "Starting"
    }
}

impl App {
    /// The whole window, every frame.
    pub(crate) fn draw(&mut self, ctx: &egui::Context) {
        self.tools = tools::current();
        // A drive search that finished.
        let found = {
            let mut f = self.finding.lock().unwrap();
            if matches!(&*f, Some(Some(_))) { f.take().flatten() } else { None }
        };
        match found {
            Some(Ok(dir)) => {
                self.saved.game_dir = dir;
                self.files.clear();
                self.find_error = None;
            }
            Some(Err(e)) => self.find_error = Some(e),
            None => {}
        }
        self.check_files();

        self.top_bar(ctx);
        self.status_bar(ctx);
        match self.view {
            View::Projects => {
                let width = (ctx.screen_rect().width() * 0.28).clamp(300.0, 380.0);
                egui::SidePanel::left("list")
                    .resizable(false)
                    .exact_width(width)
                    .frame(Frame::none().fill(theme::DEEP).inner_margin(Margin { left: 16.0, right: 16.0, top: 18.0, bottom: 0.0 }))
                    .show(ctx, |ui| self.list(ui, ctx));
                egui::CentralPanel::default().frame(Frame::none().fill(theme::BG)).show(ctx, |ui| self.page(ui, ctx));
            }
            View::Settings => {
                egui::CentralPanel::default().frame(Frame::none().fill(theme::BG)).show(ctx, |ui| self.settings_page(ui, ctx));
            }
        }
        self.auto_check(ctx);
        self.dialogs(ctx);
    }

    /// The sunset strip: the name, the two pages, and adding a repo.
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(56.0)
            .show_separator_line(false)
            .frame(Frame::none().inner_margin(Margin::symmetric(24.0, 0.0)))
            .show(ctx, |ui| {
                theme::strip(ui.painter(), ui.max_rect() + Margin::symmetric(24.0, 0.0));
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("MANDYHUB").font(theme::deco(16.0)).color(theme::PLUM).extra_letter_spacing(3.8));
                    ui.add_space(22.0);
                    if let Some(i) = theme::tabs(ui, "views", &["Projects", "Settings"], self.view as usize) {
                        self.view = if i == 0 { View::Projects } else { View::Settings };
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::button(ui, Look::Night, Some(glyph::ADD), "Add repo").clicked() {
                            self.open_add_repo();
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let running = self.job.lock().unwrap().running;
        egui::TopBottomPanel::bottom("status")
            .exact_height(42.0)
            .frame(Frame::none().fill(theme::DEEP).inner_margin(Margin::symmetric(20.0, 0.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let small = |ui: &mut Ui, text: &str, color: Color32| {
                        ui.label(RichText::new(text).font(theme::regular(13.0)).color(color));
                    };
                    if self.files_ok() {
                        theme::dot(ui, theme::GOOD);
                        small(ui, "Game files found", theme::MUTED);
                        let dir = self.saved.game_dir.clone();
                        ui.allocate_ui(vec2(300.0, 20.0), |ui| {
                            ui.add(Label::new(RichText::new(dir).font(theme::regular(12.5)).color(theme::FAINT)).truncate());
                        });
                    } else {
                        theme::dot(ui, theme::BAD);
                        small(ui, "Game files not found", theme::WHITE);
                        if theme::small(ui, Look::Ghost, Some(glyph::FOLDER), "Choose folder", true).clicked() {
                            self.pick_game_folder();
                        }
                    }
                    theme::divider(ui);
                    match self.tools.clone() {
                        None => {
                            ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                            small(ui, "Checking build tools", theme::FAINT);
                        }
                        Some(t) if t.ready() => {
                            theme::dot(ui, theme::GOOD);
                            small(ui, "Build tools ready", theme::MUTED);
                        }
                        Some(t) => {
                            theme::dot(ui, theme::WARN);
                            small(ui, &format!("Missing {}", t.missing().join(", ")), theme::WHITE);
                            if theme::small(ui, Look::Ghost, Some(glyph::DOWNLOAD), "Install tools", !running).clicked() {
                                self.start_tools(ctx);
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = format!("Graphics: {}", self.mode_name());
                        if theme::small(ui, Look::Ghost, Some(glyph::SETTINGS), &label, true).clicked() {
                            self.view = View::Settings;
                        }
                    });
                });
            });
    }

    fn list(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        // The controls stop where the rows stop: the 10 px past that is the scroll bar's.
        ui.horizontal(|ui| {
            let w = ui.available_width() - 10.0 - theme::CONTROL_H - ui.spacing().item_spacing.x;
            theme::field(ui, &mut self.search, "Search projects", w, Some(glyph::SEARCH));
            if theme::icon_button(ui, Look::Secondary, Size::Normal, glyph::REFRESH, "Reload the list from PortWatch").clicked() {
                self.refresh_projects(ctx);
            }
        });
        let w = ui.available_width() - 10.0;
        if let Some(i) = theme::segmented(ui, "gpu", &["All", "NVIDIA", "AMD"], Some(self.saved.gpu_filter.min(2)), Some(w)) {
            self.saved.gpu_filter = i;
        }

        let query = self.search.trim().to_lowercase();
        let filter = self.saved.gpu_filter;
        let keep = |p: &Project| {
            let text = query.is_empty() || p.name.to_lowercase().contains(&query) || p.repo.to_lowercase().contains(&query);
            let card = |name: &str| p.custom || p.gpu.iter().any(|g| g.eq_ignore_ascii_case(name));
            text && match filter {
                1 => card("nvidia"),
                2 => card("amd"),
                _ => true,
            }
        };
        let custom: Vec<Project> = self.saved.custom.iter().filter(|p| keep(p)).cloned().collect();
        let listed = self.projects.lock().unwrap().clone();
        let (mut pick, mut retry) = (None, false);
        let out = egui::ScrollArea::vertical().id_salt("list").auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if !custom.is_empty() {
                group(ui, "Your repos", Some(custom.len()));
                for p in &custom {
                    if self.row(ui, p).clicked() {
                        pick = Some(p.repo.clone());
                    }
                }
                ui.add_space(14.0);
            }
            match &listed {
                None => {
                    group(ui, "From PortWatch", None);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                        note(ui, "Loading the list");
                    });
                }
                Some(Err(e)) => {
                    group(ui, "From PortWatch", None);
                    theme::callout(ui, theme::BAD, glyph::WARNING, |ui| {
                        wrapped(ui, e, theme::MUTED);
                        if theme::small(ui, Look::Secondary, Some(glyph::REFRESH), "Try again", true).clicked() {
                            retry = true;
                        }
                    });
                }
                Some(Ok(list)) => {
                    let shown: Vec<&Project> = list.iter().filter(|p| keep(p)).collect();
                    group(ui, "From PortWatch", Some(shown.len()));
                    if shown.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            note(ui, "No projects match.");
                        });
                    }
                    for p in shown {
                        if self.row(ui, p).clicked() {
                            pick = Some(p.repo.clone());
                        }
                    }
                }
            }
            ui.add_space(16.0);
        });
        // While more rows are below, the list fades out at the bottom instead of stopping hard.
        let r = out.inner_rect;
        if out.content_size.y - out.state.offset.y - r.height() > 1.0 {
            let fade = egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - 40.0), r.right_bottom());
            theme::fade_bottom(ui.painter(), fade, theme::DEEP);
        }
        if let Some(r) = pick {
            self.saved.selected = Some(r);
        }
        if retry {
            self.refresh_projects(ctx);
        }
    }

    /// One project in the list: its name, frame rate, repo and where it stands.
    fn row(&self, ui: &mut Ui, p: &Project) -> egui::Response {
        let selected = self.saved.selected.as_deref() == Some(p.repo.as_str());
        // A gutter on the right keeps the scroll bar off the rows.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 10.0, 60.0), Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let ctx = ui.ctx().clone();
        let hovered = resp.hovered();
        let t = ctx.animate_bool_with_time(resp.id.with("hover"), hovered, if hovered { 0.15 } else { 0.12 });
        let sel = ctx.animate_bool_with_time(resp.id.with("sel"), selected, 0.15);
        let painter = ui.painter().clone();
        // One quiet card for the picked row; hover is a lighter step of the same.
        let fill = theme::mix(theme::mix(Color32::TRANSPARENT, theme::SURFACE, t), theme::RAISED, sel);
        painter.rect(rect, theme::R, fill, Stroke::new(1.0, theme::LINE_STRONG.gamma_multiply(sel)));
        let (left, right) = (rect.left() + 14.0, rect.right() - 16.0);

        // Right: the frame rate as a figure, set like the page's stats; a dash when none was measured.
        let fps = match p.fps {
            Some(f) if f >= 0.5 => Some((format!("{f:.0}"), theme::mix(theme::WHITE, theme::PINK, sel))),
            Some(_) => Some(("—".to_string(), theme::FAINT)),
            None => None,
        };
        let fps_w = fps.as_ref().map_or(0.0, |(text, color)| {
            let num = painter.layout_no_wrap(text.clone(), theme::display(24.0), *color);
            let unit = painter.layout_no_wrap("FPS".into(), theme::deco(9.0), theme::FAINT);
            let w = num.size().x.max(unit.size().x);
            let cx = right - w / 2.0;
            painter.galley(egui::pos2(cx - num.size().x / 2.0, rect.top() + 9.0), num, *color);
            painter.galley(egui::pos2(cx - unit.size().x / 2.0, rect.top() + 39.0), unit, theme::FAINT);
            w + 14.0
        });

        // Left: the name, with a tick when PortWatch verified it.
        let tick_w = if p.verified && theme::has_icons() { 18.0 } else { 0.0 };
        let name = theme::one_line(ui, &p.name, theme::strong(14.5), theme::WHITE, right - left - fps_w - tick_w);
        let name_size = name.size();
        let y1 = rect.top() + 10.0;
        painter.galley(egui::pos2(left, y1), name, theme::WHITE);
        if tick_w > 0.0 {
            let at = egui::pos2(left + name_size.x + 6.0, y1 + name_size.y / 2.0 + 1.0);
            let tick = painter.text(at, egui::Align2::LEFT_CENTER, glyph::CHECK, theme::glyphs(11.0), theme::GOOD);
            ui.interact(tick.expand(4.0), resp.id.with("tick"), Sense::hover()).on_hover_text("Verified by PortWatch");
        }

        // Under it, one line: where it stands when that matters, else its repo.
        let y2 = rect.top() + 34.0;
        match self.row_status(p) {
            Some((status, color)) => {
                let g = painter.layout_job(egui::text::LayoutJob::single_section(
                    status.to_uppercase(),
                    egui::TextFormat { font_id: theme::deco(10.0), color, extra_letter_spacing: 1.4, ..Default::default() },
                ));
                painter.galley(egui::pos2(left, y2 + 2.0), g, color);
            }
            None => {
                let repo = theme::one_line(ui, &p.repo, theme::regular(12.0), theme::FAINT, right - left - fps_w);
                painter.galley(egui::pos2(left, y2), repo, theme::FAINT);
            }
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Where a project stands, when that is more than "listed".
    fn row_status(&self, p: &Project) -> Option<(String, Color32)> {
        {
            let j = self.job.lock().unwrap();
            if j.running && j.repo.as_deref() == Some(p.repo.as_str()) {
                return Some((short_stage(&j.stage).to_string(), theme::PINK));
            }
        }
        if self.update_for(p).is_some() {
            return Some(("Update ready".into(), theme::WARN));
        }
        if self.state_of(&p.repo).is_some_and(|s| s.installed) {
            return Some(("Installed".into(), theme::GOOD));
        }
        None
    }

    fn empty_page(&self, ui: &mut Ui) {
        theme::ground(ui.painter(), ui.max_rect());
        let h = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.add_space((h * 0.3).max(40.0));
            let (mark, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            theme::mark(ui.painter(), mark.center(), 22.0);
            ui.add_space(14.0);
            let w = ui.available_width();
            theme::title(ui, "Pick a project", 40.0, w);
            note(ui, "Choose one from the list to see how it runs, then install it.");
        });
    }

    /// The selected project's page.
    fn page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(p) = self.selected_project() else { return self.empty_page(ui) };
        let now = ctx.input(|i| i.time);
        if self.shown.as_deref() != Some(p.repo.as_str()) {
            self.shown = Some(p.repo.clone());
            self.shown_at = now;
        }
        // The page comes in once when picked: it fades up 10 px into place.
        let enter = theme::ease_out(((now - self.shown_at) / 0.26) as f32);
        if enter < 1.0 {
            ctx.request_repaint();
        }
        let playing = {
            let j = self.job.lock().unwrap();
            j.running && j.stage == "Playing"
        };
        theme::ground(ui.painter(), ui.max_rect());
        egui::ScrollArea::vertical().id_salt("page").auto_shrink([false; 2]).show(ui, |ui| {
            let pictures = self.slides.ready(ctx);
            if pictures {
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 200.0), Sense::hover());
                self.slides.paint_in(ui.painter(), rect, now as f32, theme::BG);
                // Still while a game runs: the graphics card is the game's.
                if !playing {
                    ctx.request_repaint_after(Duration::from_millis(40));
                }
            }
            let top = if pictures { 0.0 } else { 40.0 };
            Frame::none().inner_margin(Margin { left: 44.0, right: 44.0, top, bottom: 44.0 }).show(ui, |ui| {
                ui.set_max_width(820.0);
                ui.set_opacity(enter);
                ui.add_space(10.0 * (1.0 - enter));
                self.page_header(ui, ctx, &p);
                self.page_body(ui, ctx, &p);
                ui.add_space(36.0);
                self.activity(ui);
            });
        });
    }

    /// Where it comes from, its name, the main action and the repo link buttons.
    fn page_header(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let (eyebrow, eyebrow_color) = if p.custom {
            ("Added by link", theme::WARN)
        } else if p.verified {
            ("Verified by PortWatch", theme::LILAC)
        } else {
            ("From PortWatch", theme::LILAC)
        };
        theme::caps(ui, eyebrow, 11.5, eyebrow_color);
        let width = ui.available_width();
        theme::title(ui, &p.name, 72.0, width);
        ui.label(RichText::new(format!("github.com/{}", p.repo)).font(theme::regular(13.5)).color(theme::MUTED));
        ui.add_space(20.0);

        let (running, job_repo, stage) = {
            let j = self.job.lock().unwrap();
            (j.running, j.repo.clone(), j.stage.clone())
        };
        let this_job = running && job_repo.as_deref() == Some(p.repo.as_str());
        let st = match self.state_of(&p.repo) {
            Some(st) => st,
            None => {
                self.check_repo(ctx, p);
                RepoState { checking: true, ..Default::default() }
            }
        };
        // While the first look runs, what is on disk already says installed or not.
        let installed = st.installed || (st.checking && p.local_exe.is_none() && built_exe(&p.repo).is_file());
        let tools = self.tools.clone();
        let tools_ok = tools.as_ref().is_some_and(|t| t.ready());
        let url = format!("https://github.com/{}", p.repo);
        let now = ctx.input(|i| i.time);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (icon, label) = if this_job {
                (None, short_stage(&stage))
            } else if installed {
                (Some(glyph::PLAY), "Play")
            } else {
                (Some(glyph::DOWNLOAD), "Install project")
            };
            let enabled = !running && if installed { self.files_ok() } else { tools_ok };
            if theme::button_ex(ui, Look::Primary, Size::Large, icon, label, enabled, this_job).clicked() {
                self.start(ctx, p.clone(), if installed { Kind::Play } else { Kind::Install });
            }
            if theme::button_ex(ui, Look::Secondary, Size::Large, Some(glyph::OPEN), "Open on GitHub", true, false)
                .on_hover_text(&url)
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab(&url));
            }
            // The copy button shows a tick for a moment after it copied.
            let copied = now - self.copied_at < 1.6;
            if copied {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            let (icon, tip) = if copied { (glyph::CHECK, "Link copied") } else { (glyph::COPY, "Copy link") };
            if theme::icon_button(ui, Look::Secondary, Size::Large, icon, tip).clicked() {
                ctx.copy_text(url.clone());
                self.copied_at = now;
            }
        });

        ui.add_space(8.0);
        let other = job_repo.as_ref().and_then(|r| self.all_projects().into_iter().find(|q| &q.repo == r)).map(|q| q.name);
        ui.horizontal_wrapped(|ui| {
            if this_job {
                note(ui, &stage);
            } else if running && stage == "Playing" {
                note(ui, "Close the running game first.");
            } else if running {
                note(ui, &format!("Waiting for {} to finish.", other.as_deref().unwrap_or("the current job")));
            } else if installed && self.files_ok() && self.saved.warm_before_play {
                note(ui, "Warms up the shaders first, then starts your installed build with your own game files.");
            } else if installed && self.files_ok() {
                note(ui, "Starts your installed build with your own game files.");
            } else if installed {
                note(ui, "Choose the folder with your game files to play.");
                if theme::small(ui, Look::Ghost, Some(glyph::FOLDER), "Choose folder", true).clicked() {
                    self.pick_game_folder();
                }
            } else if tools.is_none() {
                note(ui, "Checking for Git, Rust and the C++ build tools.");
            } else if tools_ok {
                note(ui, "Downloads and builds it on this PC once. The first build takes several minutes.");
            } else {
                let missing = tools.as_ref().map(|t| t.missing().join(", ")).unwrap_or_default();
                note(ui, &format!("Installing needs {missing}."));
                if theme::small(ui, Look::Ghost, Some(glyph::DOWNLOAD), "Install tools", true).clicked() {
                    self.start_tools(ctx);
                }
            }
        });

        if installed {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if st.checking {
                    ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                    note(ui, "Checking for updates");
                } else {
                    theme::dot(ui, theme::GOOD);
                    let v = st.built.as_deref().map(|c| &c[..c.len().min(7)]).unwrap_or("unknown");
                    ui.label(RichText::new(format!("Installed, version {v}")).font(theme::regular(13.0)).color(theme::MUTED));
                }
                if p.local_exe.is_none() {
                    ui.add_space(6.0);
                    if theme::small(ui, Look::Ghost, Some(glyph::SYNC), "Check for updates", !st.checking).clicked() {
                        self.check_repo(ctx, p);
                    }
                    let warm = theme::small(ui, Look::Ghost, Some(glyph::WARM), "Warm up shaders", !running && self.files_ok())
                        .on_hover_text("Translate the game's shaders into this project's cache now, so it does not stop to translate them while you play.");
                    if warm.clicked() {
                        self.start(ctx, p.clone(), Kind::Warm);
                    }
                    if theme::small(ui, Look::Ghost, Some(glyph::REPAIR), "Reinstall", !running).clicked() {
                        self.start(ctx, p.clone(), Kind::Install);
                    }
                    if theme::small(ui, Look::Danger, Some(glyph::DELETE), "Uninstall", !running).clicked() {
                        self.confirm_uninstall = Some(p.repo.clone());
                    }
                }
            });
        }
    }

    /// Update, the figures from PortWatch, and what the project is.
    fn page_body(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let running = self.job.lock().unwrap().running;
        if let Some(remote) = self.update_for(p) {
            ui.add_space(18.0);
            theme::callout(ui, theme::WARN, glyph::SYNC, |ui| {
                let short = &remote[..remote.len().min(7)];
                wrapped(ui, &format!("A new version is out: {short}."), theme::WHITE);
                ui.horizontal(|ui| {
                    if theme::small(ui, Look::Secondary, Some(glyph::DOWNLOAD), "Update now", !running).clicked() {
                        self.start(ctx, p.clone(), Kind::Install);
                    }
                    if theme::small(ui, Look::Ghost, None, "Later", true).clicked() {
                        self.saved.later.retain(|(r, _)| r != &p.repo);
                        self.saved.later.push((p.repo.clone(), remote.clone()));
                    }
                });
            });
        }

        if p.custom {
            ui.add_space(18.0);
            let mut remove = false;
            theme::callout(ui, theme::WARN, glyph::WARNING, |ui| {
                wrapped(
                    ui,
                    "You added this repo by link. Mandyhub downloads, builds and runs its code on your PC, so keep only repos from people you trust.",
                    theme::BLUSH,
                );
                if theme::small(ui, Look::Danger, Some(glyph::DELETE), "Remove from list", true).clicked() {
                    remove = true;
                }
            });
            if remove {
                self.saved.custom.retain(|q| q.repo != p.repo);
                self.saved.selected = None;
            }
            return;
        }

        ui.add_space(30.0);
        let gpu = gpu_text(&p.gpu);
        theme::stats(
            ui,
            &[
                Stat {
                    label: "Frame rate",
                    value: p.fps.map_or("Not reported".into(), |f| format!("{f:.0} fps")),
                    color: theme::WHITE,
                    tip: Some("The average in game on the author's PC."),
                },
                Stat {
                    label: "Rendering",
                    value: p.render.map_or("Not reported".into(), |r| format!("{:.0}%", r * 100.0)),
                    color: theme::WHITE,
                    tip: None,
                },
                Stat {
                    label: "Made for",
                    value: if gpu.is_empty() { "Not stated".into() } else { gpu },
                    color: theme::WHITE,
                    tip: None,
                },
                Stat {
                    label: "PortWatch",
                    value: if p.verified { "Verified" } else { "Not verified" }.into(),
                    color: if p.verified { theme::GOOD } else { theme::MUTED },
                    tip: None,
                },
            ],
        );
        if !p.gpu.is_empty() && p.gpu.iter().all(|g| !g.eq_ignore_ascii_case("nvidia")) {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                theme::icon(ui, glyph::WARNING, 13.0, theme::WARN);
                ui.label(RichText::new("Made for AMD cards: it may not start on NVIDIA.").font(theme::regular(13.0)).color(theme::MUTED));
            });
        }
        if !p.summary.trim().is_empty() {
            ui.add_space(32.0);
            theme::caps(ui, "About", 11.5, theme::LILAC);
            ui.add_space(2.0);
            // Lines of a readable length, whatever the window's width.
            ui.scope(|ui| {
                ui.set_max_width(680.0);
                ui.add(Label::new(RichText::new(&p.summary).font(theme::regular(15.0)).color(theme::BLUSH)).wrap());
            });
        }
    }

    /// What the launcher is doing, and its log.
    fn activity(&mut self, ui: &mut Ui) {
        let (running, failed, stage, has_log, stoppable) = {
            let j = self.job.lock().unwrap();
            (j.running, j.failed, j.stage.clone(), !j.log.is_empty(), j.running && j.stoppable)
        };
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
            theme::caps(ui, "Activity", 11.5, theme::LILAC);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if has_log {
                    let label = if self.show_log { "Hide log" } else { "Show log" };
                    if theme::small(ui, Look::Ghost, None, label, true).clicked() {
                        self.show_log = !self.show_log;
                    }
                }
                if stoppable && theme::small(ui, Look::Danger, Some(glyph::STOP), "Stop", true).clicked() {
                    self.stop_job();
                }
            });
        });
        ui.horizontal(|ui| {
            if running && stage != "Playing" {
                ui.add(egui::Spinner::new().size(12.0).color(theme::PINK));
            } else {
                theme::dot(ui, if failed { theme::BAD } else if stage.is_empty() { theme::LINE_STRONG } else { theme::GOOD });
            }
            let (text, color) = if stage.is_empty() {
                ("Nothing running".to_string(), theme::FAINT)
            } else if failed {
                (stage, theme::BAD)
            } else if running {
                (stage, theme::WHITE)
            } else {
                (stage, theme::MUTED)
            };
            ui.label(RichText::new(text).font(theme::regular(13.5)).color(color));
        });
        if has_log && (self.show_log || failed) {
            ui.add_space(6.0);
            Frame::none()
                .fill(theme::DEEP)
                .stroke(Stroke::new(1.0, theme::LINE))
                .rounding(theme::R)
                .inner_margin(Margin::same(14.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("log").max_height(240.0).stick_to_bottom(true).auto_shrink([false, true]).show(
                        ui,
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let j = self.job.lock().unwrap();
                            for line in j.log.iter().rev().take(400).rev() {
                                let color = if line.starts_with("ERROR") {
                                    theme::BAD
                                } else if line.starts_with("== ") {
                                    theme::WHITE
                                } else {
                                    theme::FAINT
                                };
                                ui.add(Label::new(RichText::new(line).font(theme::mono(12.0)).color(color)).wrap());
                            }
                        },
                    );
                });
        }
    }

    fn settings_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::ground(ui.painter(), ui.max_rect());
        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false; 2]).show(ui, |ui| {
            let avail = ui.available_width();
            let col = (avail - 64.0).min(720.0);
            let side = ((avail - col) / 2.0).max(32.0);
            Frame::none().inner_margin(Margin { left: side, right: side, top: 40.0, bottom: 48.0 }).show(ui, |ui| {
                ui.set_width(col);
                theme::caps(ui, "Mandyhub", 11.5, theme::LILAC);
                theme::title(ui, "Settings", 64.0, col);
                ui.add_space(18.0);
                self.game_files_card(ui, ctx);
                ui.add_space(16.0);
                self.graphics_card(ui);
                ui.add_space(16.0);
                self.launch_card(ui);
                ui.add_space(16.0);
                self.install_folder_card(ui);
                ui.add_space(16.0);
                self.pictures_card(ui);
                ui.add_space(16.0);
                self.switches_card(ui);
                ui.add_space(16.0);
                self.about_card(ui, ctx);
            });
        });
    }

    fn game_files_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let searching = matches!(&*self.finding.lock().unwrap(), Some(None));
        section(
            ui,
            "Game files",
            None,
            "The folder with game_scarlett_master.exe from your own copy of the game. Mandyhub never downloads game files.",
            |ui| {
                ui.allocate_ui_with_layout(vec2(ui.available_width(), theme::CONTROL_H), Layout::right_to_left(Align::Center), |ui| {
                    let label = if searching { "Searching drives" } else { "Find game files" };
                    if theme::button_ex(ui, Look::Ghost, Size::Normal, Some(glyph::SEARCH), label, !searching, searching).clicked() {
                        self.find_error = None;
                        *self.finding.lock().unwrap() = Some(None);
                        let (slot, ctx) = (self.finding.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let r = search_drives();
                            *slot.lock().unwrap() = Some(Some(r));
                            ctx.request_repaint();
                        });
                    }
                    if theme::button(ui, Look::Secondary, Some(glyph::FOLDER), "Choose folder").clicked() {
                        self.pick_game_folder();
                    }
                    let w = ui.available_width();
                    if theme::field(ui, &mut self.saved.game_dir, "Paste the folder's path", w, None).changed() {
                        self.files.clear();
                    }
                });
                ui.add_space(10.0);
                self.files_status(ui);
                if let Some(e) = &self.find_error {
                    ui.add_space(4.0);
                    wrapped(ui, e, theme::BAD);
                }
            },
        );
    }

    fn files_status(&self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.saved.game_dir.trim().is_empty() {
                note(ui, "No folder chosen yet.");
            } else if self.files_ok() {
                theme::icon(ui, glyph::CHECK, 13.0, theme::GOOD);
                ui.label(RichText::new("All game files are here.").font(theme::regular(13.0)).color(theme::GOOD));
            } else if self.empty {
                theme::icon(ui, glyph::WARNING, 13.0, theme::BAD);
                ui.label(RichText::new("This folder is empty.").font(theme::regular(13.0)).color(theme::BAD));
            } else {
                theme::icon(ui, glyph::WARNING, 13.0, theme::BAD);
                ui.label(RichText::new("Missing").font(theme::regular(13.0)).color(theme::BAD));
                for (f, ok) in &self.files {
                    if !ok {
                        ui.label(RichText::new(*f).font(theme::mono(12.0)).color(theme::BLUSH));
                    }
                }
            }
        });
    }

    fn graphics_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Graphics",
            Some("Beta"),
            "These options may not work with every project or on older PCs. If a game will not start or looks wrong, choose Best quality and turn the fixes off.",
            |ui| {
                let current = self.mode_index();
                ui.horizontal(|ui| {
                    let names: Vec<&str> = MODES.iter().map(|m| m.0).collect();
                    if let Some(i) = theme::segmented(ui, "mode", &names, current, None) {
                        self.saved.series_s = MODES[i].1;
                        self.saved.render_scale = MODES[i].2;
                    }
                    ui.add_space(6.0);
                    let what = match self.mode_index() {
                        Some(i) => MODES[i].3.to_string(),
                        None => format!("Your own render scale: {:.0}%.", self.saved.render_scale * 100.0),
                    };
                    note(ui, &what);
                });
                ui.add_space(20.0);
                theme::toggle(ui, &mut self.saved.fix_8gb, "8 GB graphics card fix", "Smaller textures, and grey in place of missing ones.");
                ui.add_space(14.0);
                theme::toggle(ui, &mut self.saved.bright_shadows, "Brighter shadows", "A steady ambient light where ray tracing does not run.");
                ui.add_space(14.0);
                theme::toggle(
                    ui,
                    &mut self.saved.fullscreen,
                    "Borderless fullscreen",
                    "The game's window covers its whole monitor, with no frame.",
                );
            },
        );
    }

    fn launch_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Shaders and launch",
            None,
            "VIctory can translate the game's shaders into its cache before you play, so the game does not stop to translate them on the way. Each project keeps its own cache.",
            |ui| {
                theme::toggle(
                    ui,
                    &mut self.saved.warm_before_play,
                    "Warm up shaders before playing",
                    "Runs the warm-up each time you press Play. The first one takes minutes and about 1.3 GB; you can also run it once from a project's page.",
                );
                ui.add_space(14.0);
                theme::toggle(
                    ui,
                    &mut self.saved.warm_pipelines,
                    "Also build the GPU pipelines",
                    "Slow: about 20 minutes on 16 threads, hours after a translator update. They belong to this graphics card and driver.",
                );
                ui.add_space(20.0);
                ui.label(RichText::new("Launch arguments").font(theme::strong(14.0)).color(theme::WHITE));
                note(ui, "Added to victory run, for example --strict. Arguments for the game itself go after --.");
                ui.add_space(6.0);
                let w = ui.available_width();
                theme::field(ui, &mut self.saved.launch_args, "--strict -- -title-argument", w, None);
            },
        );
    }

    fn install_folder_card(&mut self, ui: &mut Ui) {
        let running = self.job.lock().unwrap().running;
        section(
            ui,
            "Install folder",
            None,
            "Projects are downloaded and built here. Each one takes several GB, so a big drive is best.",
            |ui| {
                ui.add(Label::new(RichText::new(workspace().display().to_string()).font(theme::mono(12.5)).color(theme::BLUSH)).truncate());
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if theme::button_ex(ui, Look::Secondary, Size::Normal, Some(glyph::FOLDER), "Change folder", !running, false).clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_title("Where projects are downloaded and built").pick_folder() {
                            self.choose_install_dir(d.join("Mandyhub"));
                        }
                    }
                    if !self.saved.install_dir.is_empty()
                        && theme::button_ex(ui, Look::Ghost, Size::Normal, None, "Use the default folder", !running, false).clicked()
                    {
                        self.choose_install_dir(default_workspace());
                    }
                });
            },
        );
    }

    fn pictures_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Background pictures",
            None,
            "Put .jpg or .png pictures in this folder and they show across the top of each project's page. Mandyhub reads them when it starts.",
            |ui| {
                let dir = slides::folder();
                ui.add(Label::new(RichText::new(dir.display().to_string()).font(theme::mono(12.5)).color(theme::BLUSH)).truncate());
                ui.add_space(10.0);
                if theme::button(ui, Look::Secondary, Some(glyph::FOLDER), "Open the folder").clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            },
        );
    }

    fn switches_card(&mut self, ui: &mut Ui) {
        section(ui, "Launch switches", None, "For experts: extra switches for the game, one per line, as VICTORY_NAME=value.", |ui| {
            let w = ui.available_width();
            ui.add(
                egui::TextEdit::multiline(&mut self.saved.extra_env)
                    .font(theme::mono(12.5))
                    .text_color(theme::WHITE)
                    .hint_text(RichText::new("VICTORY_NAME=value").color(theme::FAINT))
                    .desired_rows(3)
                    .desired_width(w - 28.0)
                    .margin(Margin::symmetric(14.0, 10.0)),
            );
        });
    }

    fn about_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        section(ui, "About Mandyhub", None, "", |ui| {
            wrapped(
                ui,
                "Mandyhub by Zettrox. The project list comes from PortWatch. The projects are their authors' work, built on VIctory by Marvisak and its contributors.",
                theme::BLUSH,
            );
            wrapped(ui, "No game files are included or downloaded: you need your own copy of the game.", theme::FAINT);
            wrapped(ui, "Display type: Josefin Sans and Big Shoulders Display, under the SIL Open Font License.", theme::FAINT);
            ui.add_space(10.0);
            if theme::button(ui, Look::Secondary, Some(glyph::OPEN), "Open portwatch-vi.xyz").clicked() {
                ctx.open_url(egui::OpenUrl::new_tab("https://portwatch-vi.xyz"));
            }
        });
    }

    /// The open dialog, if any: add a repo, uninstall, or move the install folder.
    fn dialogs(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let open = self.add_repo.is_some() || self.confirm_uninstall.is_some() || self.pending_move.is_some();
        if open && !self.modal_was_open {
            self.modal_at = now;
        }
        self.modal_was_open = open;
        let t0 = self.modal_at;

        if let Some(mut add) = self.add_repo.take() {
            let mut keep = true;
            let (_, dismissed) = theme::modal(ctx, "add-repo", t0, 460.0, |ui| {
                theme::caps(ui, "Custom repo", 11.5, theme::LILAC);
                theme::title(ui, "Add a repo by link", 40.0, 460.0);
                note(ui, "Paste the GitHub link of a VIctory-based project.");
                ui.add_space(14.0);
                let w = ui.available_width();
                let field = theme::field(ui, &mut add.link, "github.com/owner/name", w, Some(glyph::LINK));
                if add.focus {
                    field.request_focus();
                    add.focus = false;
                }
                if field.changed() {
                    add.error = None;
                }
                let mut submit = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(e) = &add.error {
                    ui.add_space(2.0);
                    wrapped(ui, e, theme::BAD);
                }
                ui.add_space(14.0);
                theme::callout(ui, theme::WARN, glyph::WARNING, |ui| {
                    wrapped(
                        ui,
                        "Mandyhub downloads, builds and runs this repo's code on your PC. Add only repos from people you trust: fake repos with malware exist.",
                        theme::BLUSH,
                    );
                });
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::Primary, Some(glyph::ADD), "Add repo").clicked() {
                        submit = true;
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        keep = false;
                    }
                });
                if submit {
                    match parse_repo(&add.link) {
                        Some(repo) => {
                            self.add_custom(repo);
                            keep = false;
                        }
                        None => add.error = Some("That is not a GitHub repo link. Paste one like github.com/owner/name.".into()),
                    }
                }
            });
            if keep && !dismissed {
                self.add_repo = Some(add);
            }
        }

        if let Some(repo) = self.confirm_uninstall.clone() {
            let project = self.all_projects().into_iter().find(|p| p.repo == repo);
            let name = project.as_ref().map_or_else(|| repo.clone(), |p| p.name.clone());
            let mut close = false;
            let (_, dismissed) = theme::modal(ctx, "uninstall", t0, 440.0, |ui| {
                theme::caps(ui, "Uninstall", 11.5, theme::BAD);
                theme::title(ui, &name, 40.0, 440.0);
                wrapped(ui, "This deletes its download, its build and its shader cache. Your saves stay, and you can install it again later.", theme::BLUSH);
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::DangerSolid, Some(glyph::DELETE), "Uninstall").clicked() {
                        if let Some(p) = project.clone() {
                            self.start(ctx, p, Kind::Uninstall);
                        }
                        close = true;
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close || dismissed {
                self.confirm_uninstall = None;
            }
        }

        if let Some((old, new)) = self.pending_move.clone() {
            let mut close = false;
            let (_, dismissed) = theme::modal(ctx, "move", t0, 520.0, |ui| {
                theme::caps(ui, "Install folder", 11.5, theme::LILAC);
                theme::title(ui, "Move installed projects?", 40.0, 520.0);
                wrapped(ui, "Your installed projects and their saves are in the old folder. Moving them to another drive can take a while.", theme::BLUSH);
                ui.add_space(10.0);
                ui.add(Label::new(RichText::new(format!("From  {}", old.display())).font(theme::mono(12.0)).color(theme::FAINT)).truncate());
                ui.add(Label::new(RichText::new(format!("To    {}", new.display())).font(theme::mono(12.0)).color(theme::WHITE)).truncate());
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::Primary, None, "Move them").clicked() {
                        self.apply_install_dir(&new);
                        self.start_move(ctx, old.clone(), new.clone());
                        close = true;
                    }
                    if theme::button(ui, Look::Secondary, None, "Don't move them").clicked() {
                        self.apply_install_dir(&new);
                        close = true;
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close || dismissed {
                self.pending_move = None;
            }
        }
    }

    /// Ends the running warm-up (its process and the workers it started).
    fn stop_job(&self) {
        let pid = {
            let mut j = self.job.lock().unwrap();
            j.stopped = true;
            j.pid
        };
        if let Some(pid) = pid {
            let mut c = std::process::Command::new("taskkill");
            c.args(["/PID", &pid.to_string(), "/T", "/F"]);
            crate::hide_window(&mut c);
            let _ = c.spawn();
        }
    }

    fn open_add_repo(&mut self) {
        self.add_repo = Some(AddRepo { link: String::new(), error: None, focus: true });
    }

    /// Puts a repo added by link at the top of the list and opens its page.
    fn add_custom(&mut self, repo: String) {
        let name = repo.split('/').next_back().unwrap_or(&repo).to_string();
        self.saved.custom.retain(|p| p.repo != repo);
        self.saved.custom.insert(
            0,
            Project {
                name,
                repo: repo.clone(),
                branch: None,
                gpu: Vec::new(),
                fps: None,
                render: None,
                verified: false,
                summary: String::new(),
                custom: true,
                local_exe: None,
            },
        );
        self.saved.selected = Some(repo);
        self.view = View::Projects;
    }

    fn pick_game_folder(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_title("The folder with your game files").pick_folder() {
            self.saved.game_dir = d.display().to_string();
            self.files.clear();
        }
    }

    fn mode_index(&self) -> Option<usize> {
        MODES.iter().position(|(_, series_s, scale, _)| {
            self.saved.series_s == *series_s && (!*series_s || (self.saved.render_scale - scale).abs() < 0.01)
        })
    }

    fn mode_name(&self) -> String {
        match self.mode_index() {
            Some(i) => MODES[i].0.to_string(),
            None => format!("{:.0}%", self.saved.render_scale * 100.0),
        }
    }
}
