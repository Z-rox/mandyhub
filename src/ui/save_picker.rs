use super::{button_row, note, wrapped};
use crate::saves::{self, LaunchSave};
use crate::theme::{self, glyph, Look, Size};
use crate::{slug, workspace, Project};
use eframe::egui::{self, vec2, Color32, RichText, Ui};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    ThisBuild,
    OtherBuilds,
    Favorites,
}

struct SaveOption {
    title: String,
    detail: &'static str,
    choice: LaunchSave,
    favorite_source: Option<PathBuf>,
}

pub(super) enum Action {
    Keep,
    Cancel,
    Launch(LaunchSave),
}

pub(crate) struct SavePicker {
    pub(crate) project: Project,
    tab: Tab,
    selected: usize,
    this_build: Vec<SaveOption>,
    other_builds: Vec<SaveOption>,
    favorites: Vec<SaveOption>,
    checkpoint_root: PathBuf,
    naming: Option<String>,
    focus_name: bool,
    error: Option<String>,
}

impl SaveOption {
    fn copy(title: String, detail: &'static str, path: PathBuf) -> Self {
        Self {
            title,
            detail,
            choice: LaunchSave::Copy(path.clone()),
            favorite_source: Some(path),
        }
    }

    fn favorite(title: String, path: PathBuf) -> Self {
        Self {
            title,
            detail: "Starts from a copy of this checkpoint.",
            choice: LaunchSave::Copy(path),
            favorite_source: None,
        }
    }
}

impl SavePicker {
    pub(crate) fn new(project: Project) -> Self {
        let ws = workspace();
        let data = ws.join("data");
        let project_data = data.join(slug(&project.repo));
        let checkpoint_root = ws.join("checkpoints");
        let current = saves::save_folder(&project_data);
        let selected = usize::from(current.is_none());
        let mut this_build = vec![
            SaveOption {
                title: "Continue".into(),
                detail: "Use this build’s active save folder.",
                choice: LaunchSave::Continue,
                favorite_source: current,
            },
            SaveOption {
                title: "Start fresh".into(),
                detail: "Begin a new game in a separate save profile.",
                choice: LaunchSave::Fresh,
                favorite_source: None,
            },
        ];
        this_build.extend(
            saves::previous_saves(&project_data)
                .into_iter()
                .map(|(name, path)| {
                    SaveOption::copy(name, "Starts from a copy of an earlier save.", path)
                }),
        );
        let mut other_builds = Vec::new();
        for entry in fs::read_dir(&data).into_iter().flatten().flatten() {
            if entry.path() != project_data {
                if let Some(path) = saves::save_folder(&entry.path()) {
                    let name = entry.file_name().to_string_lossy().replacen('_', "/", 1);
                    other_builds.push(SaveOption::copy(
                        name,
                        "Copies this build’s active save.",
                        path,
                    ));
                }
            }
        }
        other_builds.sort_by(|a, b| a.title.cmp(&b.title));
        let mut favorites = Vec::new();
        for entry in fs::read_dir(&checkpoint_root)
            .into_iter()
            .flatten()
            .flatten()
        {
            let filename = entry.file_name();
            if let Some(name) = filename
                .to_str()
                .and_then(|name| name.strip_prefix("Checkpoint - "))
            {
                if entry.path().is_dir() {
                    favorites.push(SaveOption::favorite(name.into(), entry.path()));
                }
            }
        }
        favorites.sort_by(|a, b| a.title.cmp(&b.title));
        Self {
            project,
            tab: Tab::ThisBuild,
            selected,
            this_build,
            other_builds,
            favorites,
            checkpoint_root,
            naming: None,
            focus_name: false,
            error: None,
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        opened_at: f64,
        can_play: bool,
    ) -> (Action, bool) {
        theme::modal(ctx, "save-picker", opened_at, 540.0, |ui| {
            theme::caps(ui, &self.project.name, 11.5, theme::LILAC);
            theme::title(ui, "Choose a save", 40.0, 540.0);
            note(ui, "Choose where to start. Fresh games and copied saves keep your earlier progress available.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::ThisBuild, "This build"),
                    (Tab::OtherBuilds, "Other builds"),
                    (Tab::Favorites, "Favorites"),
                ] {
                    if theme::button(
                        ui,
                        if self.tab == tab {
                            Look::Primary
                        } else {
                            Look::Ghost
                        },
                        None,
                        label,
                    )
                    .clicked()
                        && self.tab != tab
                    {
                        self.tab = tab;
                        self.selected = 0;
                        self.naming = None;
                        self.error = None;
                    }
                }
            });
            ui.add_space(8.0);
            let (options, empty) = match self.tab {
                Tab::ThisBuild => (&self.this_build, ""),
                Tab::OtherBuilds => (
                    &self.other_builds,
                    "Saves from other builds appear here after you save in them.",
                ),
                Tab::Favorites => (
                    &self.favorites,
                    "To create a checkpoint, choose an existing save and select Add to favorites.",
                ),
            };
            let mut selected = self.selected;
            egui::ScrollArea::vertical()
                .id_salt("save-list")
                .max_height(170.0)
                .show(ui, |ui| {
                    if options.is_empty() {
                        wrapped(ui, empty, theme::FAINT);
                    }
                    for (index, option) in options.iter().enumerate() {
                        if save_row(ui, option, selected == index) {
                            selected = index;
                        }
                    }
                });
            if selected != self.selected {
                self.selected = selected;
                self.naming = None;
                self.error = None;
            }
            let choice = options.get(selected).map(|option| option.choice.clone());
            let source = options
                .get(selected)
                .and_then(|option| option.favorite_source.clone());
            if let Some(option) = options.get(selected) {
                ui.add_space(4.0);
                note(ui, option.detail);
            }
            if let (Some(name), Some(source)) = (&mut self.naming, &source) {
                ui.add_space(10.0);
                note(ui, "Give this checkpoint a name you’ll remember.");
                let width = ui.available_width();
                let field = theme::field(ui, name, "e.g. Intro complete", width, None);
                if self.focus_name {
                    field.request_focus();
                    self.focus_name = false;
                }
                if field.changed() {
                    self.error = None;
                }
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(error) = &self.error {
                    wrapped(ui, error, theme::BAD);
                }
                let mut action = Action::Keep;
                button_row(ui, |ui| {
                    let valid = !name.trim().is_empty();
                    let save = theme::button_ex(
                        ui,
                        Look::Primary,
                        Size::Normal,
                        Some(glyph::CHECK),
                        "Save checkpoint",
                        valid,
                        false,
                    )
                    .clicked();
                    if valid && (enter || save) {
                        match saves::checkpoint(&self.checkpoint_root, name, source) {
                            Ok(path) => {
                                let checkpoint = SaveOption::favorite(name.trim().into(), path);
                                let index = self
                                    .favorites
                                    .partition_point(|option| option.title < checkpoint.title);
                                self.favorites.insert(index, checkpoint);
                                self.tab = Tab::Favorites;
                                self.selected = index;
                                self.error = None;
                                action = Action::Cancel;
                            }
                            Err(error) => {
                                self.error = Some(format!("Could not save checkpoint: {error}"))
                            }
                        }
                    }
                    if theme::button(ui, Look::Ghost, None, "Back").clicked() {
                        action = Action::Cancel;
                    }
                });
                if matches!(action, Action::Cancel) {
                    self.naming = None;
                    self.error = None;
                }
                return Action::Keep;
            }
            ui.add_space(12.0);
            let mut action = Action::Keep;
            button_row(ui, |ui| {
                let label = match &choice {
                    Some(LaunchSave::Continue) => "Continue game",
                    Some(LaunchSave::Fresh) => "Start new game",
                    Some(LaunchSave::Copy(_)) => "Play from copy",
                    None => "Play",
                };
                if theme::button_ex(
                    ui,
                    Look::Primary,
                    Size::Normal,
                    Some(glyph::PLAY),
                    label,
                    can_play && choice.is_some(),
                    false,
                )
                .clicked()
                {
                    if let Some(choice) = choice {
                        action = Action::Launch(choice);
                    }
                }
                if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                    action = Action::Cancel;
                }
                if source.is_some()
                    && theme::button(ui, Look::Secondary, Some(glyph::ADD), "Add to favorites")
                        .clicked()
                {
                    self.naming = Some(String::new());
                    self.focus_name = true;
                }
            });
            action
        })
    }
}

fn save_row(ui: &mut Ui, option: &SaveOption, selected: bool) -> bool {
    let title = RichText::new(format!(
        "{}  {}",
        if selected { "●" } else { "○" },
        option.title
    ))
    .font(theme::regular(15.0))
    .color(theme::WHITE);
    let row = egui::Button::new(title)
        .min_size(vec2(ui.available_width(), 34.0))
        .fill(if selected {
            theme::LINE_STRONG
        } else {
            Color32::TRANSPARENT
        });
    ui.add(row).clicked()
}
