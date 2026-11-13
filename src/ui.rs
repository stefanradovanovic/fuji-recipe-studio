use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2,
};
use fuji_recipe_studio::{
    engine::{self, Frame},
    import::{self, Imported},
    recipe::{self, Recipe},
};
use image::RgbImage;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(17, 21, 19);
const PANEL: Color32 = Color32::from_rgb(25, 31, 27);
const CARD: Color32 = Color32::from_rgb(34, 42, 36);
const ACCENT: Color32 = Color32::from_rgb(191, 218, 154);
const MUTED: Color32 = Color32::from_rgb(144, 158, 147);

type Loaded = (Arc<Frame>, Arc<Frame>, RgbImage);
enum Event {
    Loaded(u64, anyhow::Result<Loaded>, Duration),
    Rendered(u64, u64, anyhow::Result<RgbImage>, Duration),
    Exported(anyhow::Result<PathBuf>, Duration),
    Imported(anyhow::Result<Imported>),
}

#[derive(Default)]
struct ImportDialog {
    open: bool,
    url: String,
    name: String,
    text: String,
    busy: bool,
    preview: Option<Imported>,
    error: String,
}

pub struct Studio {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    recipes: Vec<Recipe>,
    selected: usize,
    current: Recipe,
    builtin_count: usize,
    library_path: PathBuf,
    library_error: bool,
    search: String,
    files: Vec<PathBuf>,
    active_file: Option<PathBuf>,
    full: Option<Arc<Frame>>,
    small: Option<Arc<Frame>>,
    neutral: Option<TextureHandle>,
    rendered: Option<TextureHandle>,
    generation: u64,
    revision: u64,
    pending_load: Option<(u64, PathBuf)>,
    loading: bool,
    render_running: Option<(u64, u64)>,
    render_due: Option<Instant>,
    exporting: bool,
    exposure: f32,
    comparison: u8,
    split: f32,
    status: String,
    error: String,
    decode_time: Duration,
    render_time: Duration,
    import: ImportDialog,
    save_open: bool,
    save_name: String,
    help_open: bool,
    capture_path: Option<PathBuf>,
    capture_requested: bool,
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = CARD;
        style.visuals.selection.bg_fill = Color32::from_rgb(66, 88, 56);
        style.visuals.selection.stroke = Stroke::new(1_f32, ACCENT);
        style.visuals.widgets.inactive.bg_fill = CARD;
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 61, 48);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(61, 78, 55);
        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(230, 235, 225);
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(14., 9.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.));
        cc.egui_ctx.set_style(style);
        let (tx, rx) = mpsc::channel();
        let mut recipes = recipe::builtins();
        let builtin_count = recipes.len();
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap_or_default()
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            });
        let library_path = base.join("FujiRecipeStudio").join("recipes.json");
        let mut error = String::new();
        let mut library_error = false;
        match recipe::load_library(&library_path) {
            Ok(custom) => recipes.extend(custom),
            Err(e) => {
                error = format!(
                    "Your saved recipe library could not be read. It has been preserved at {}. {e}",
                    library_path.display()
                );
                library_error = true;
            }
        }
        let mut app = Self {
            tx,
            rx,
            current: recipes[1].clone(),
            recipes,
            selected: 1,
            builtin_count,
            library_path,
            library_error,
            search: String::new(),
            files: Vec::new(),
            active_file: None,
            full: None,
            small: None,
            neutral: None,
            rendered: None,
            generation: 0,
            revision: 0,
            pending_load: None,
            loading: false,
            render_running: None,
            render_due: None,
            exporting: false,
            exposure: 0.,
            comparison: 1,
            split: 0.5,
            status: "Open a photo to begin".into(),
            error,
            decode_time: Duration::ZERO,
            render_time: Duration::ZERO,
            import: ImportDialog::default(),
            save_open: false,
            save_name: String::new(),
            help_open: false,
            capture_path: None,
            capture_requested: false,
        };
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--capture" && i + 1 < args.len() {
                i += 1;
                app.capture_path = Some(PathBuf::from(&args[i]));
            } else if args[i] == "--compare" {
                app.comparison = 2;
            } else {
                let p = PathBuf::from(&args[i]);
                if p.is_file() {
                    app.files.push(p);
                }
            }
            i += 1;
        }
        if let Some(path) = app.files.first().cloned() {
            app.queue_load(path);
        }
        app
    }

    fn texture(ctx: &egui::Context, name: &str, image: RgbImage) -> TextureHandle {
        let size = [image.width() as usize, image.height() as usize];
        ctx.load_texture(
            name,
            egui::ColorImage::from_rgb(size, image.as_raw()),
            egui::TextureOptions::LINEAR,
        )
    }

    fn queue_load(&mut self, path: PathBuf) {
        self.generation += 1;
        self.revision += 1;
        self.pending_load = Some((self.generation, path.clone()));
        self.active_file = Some(path);
        self.full = None;
        self.small = None;
        self.neutral = None;
        self.rendered = None;
        self.exposure = 0.;
        self.render_due = None;
        self.status = "Developing RAW…".into();
        self.error.clear();
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.render_due = Some(Instant::now() + Duration::from_millis(35));
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let supported = |p: &PathBuf| {
            p.extension().and_then(|v| v.to_str()).is_some_and(|e| {
                ["raf", "jpg", "jpeg", "png", "tif", "tiff"]
                    .contains(&e.to_ascii_lowercase().as_str())
            })
        };
        let valid: Vec<_> = paths.into_iter().filter(supported).collect();
        let first = valid.first().cloned();
        for path in valid {
            if !self.files.contains(&path) {
                self.files.push(path);
            }
        }
        if let Some(path) = first {
            self.queue_load(path);
        } else {
            self.error = "Choose an X-Trans .RAF, JPEG, PNG or TIFF image.".into();
        }
    }

    fn open_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Open photographs")
            .add_filter("Photographs", &["raf", "jpg", "jpeg", "png", "tif", "tiff"])
            .pick_files()
        {
            self.add_files(paths);
        }
    }

    fn export(&mut self, ctx: &egui::Context) {
        if self.exporting {
            return;
        }
        let (Some(frame), Some(source)) = (self.full.clone(), self.active_file.clone()) else {
            return;
        };
        let safe_name: String = self
            .current
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let stem = source.file_stem().unwrap_or_default().to_string_lossy();
        let destination = rfd::FileDialog::new()
            .set_title("Export a new image")
            .set_directory(source.parent().unwrap_or(std::path::Path::new(".")))
            .set_file_name(format!("{stem}_{safe_name}.jpg"))
            .add_filter("JPEG", &["jpg"])
            .add_filter("PNG", &["png"])
            .save_file();
        if let Some(destination) = destination {
            self.exporting = true;
            self.status = "Exporting full resolution…".into();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            let recipe = self.current.clone();
            let exposure = self.exposure;
            std::thread::spawn(move || {
                let start = Instant::now();
                let result =
                    guarded(|| engine::export(&frame, &source, &destination, &recipe, exposure));
                let _ = tx.send(Event::Exported(result, start.elapsed()));
                ctx.request_repaint();
            });
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Loaded(generation, result, duration) => {
                    self.loading = false;
                    if generation != self.generation {
                        continue;
                    }
                    match result {
                        Ok((full, small, neutral)) => {
                            self.full = Some(full);
                            self.small = Some(small);
                            self.neutral = Some(Self::texture(ctx, "neutral", neutral));
                            self.decode_time = duration;
                            self.status = "RAW ready".into();
                            self.changed();
                        }
                        Err(e) => {
                            self.error = format!("Could not open photo: {e:#}");
                            self.status = "Choose another image".into();
                        }
                    }
                }
                Event::Rendered(generation, revision, result, duration) => {
                    if self.render_running == Some((generation, revision)) {
                        self.render_running = None;
                    }
                    if generation == self.generation && revision == self.revision {
                        match result {
                            Ok(image) => {
                                self.rendered = Some(Self::texture(ctx, "recipe", image));
                                self.render_time = duration;
                                if !self.exporting {
                                    self.status = "Preview ready".into();
                                }
                            }
                            Err(e) => self.error = format!("Preview failed: {e:#}"),
                        }
                    }
                }
                Event::Exported(result, duration) => {
                    self.exporting = false;
                    match result {
                        Ok(path) => {
                            self.status = format!(
                                "Saved {} · {:.1} s",
                                path.display(),
                                duration.as_secs_f32()
                            )
                        }
                        Err(e) => {
                            self.error = format!("Export failed: {e:#}");
                            self.status = "Export was not completed".into();
                        }
                    }
                }
                Event::Imported(result) => {
                    self.import.busy = false;
                    match result {
                        Ok(imported) => {
                            self.import.preview = Some(imported);
                            self.import.error.clear();
                        }
                        Err(e) => self.import.error = format!("{e:#}"),
                    }
                }
            }
        }
        if !self.loading {
            if let Some((generation, path)) = self.pending_load.take() {
                self.loading = true;
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let result = guarded(|| {
                        let full = Arc::new(engine::decode(&path)?);
                        let small = Arc::new(engine::preview(&full, 1600));
                        let neutral = engine::render(&small, &Recipe::default(), 0.)?;
                        Ok((full, small, neutral))
                    });
                    let _ = tx.send(Event::Loaded(generation, result, start.elapsed()));
                    ctx.request_repaint();
                });
            }
        }
        if self.render_running.is_none() && self.render_due.is_some_and(|due| due <= Instant::now())
        {
            if let Some(frame) = self.small.clone() {
                self.render_due = None;
                let generation = self.generation;
                let revision = self.revision;
                self.render_running = Some((generation, revision));
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                let recipe = self.current.clone();
                let exposure = self.exposure;
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let result = guarded(|| engine::render(&frame, &recipe, exposure));
                    let _ = tx.send(Event::Rendered(
                        generation,
                        revision,
                        result,
                        start.elapsed(),
                    ));
                    ctx.request_repaint();
                });
            }
        }
        if self.loading
            || self.render_running.is_some()
            || self.render_due.is_some()
            || self.exporting
            || self.import.busy
        {
            ctx.request_repaint_after(Duration::from_millis(35));
        }
    }

    fn save_custom(&mut self, recipe: Recipe) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.library_error,
            "Existing library is unreadable and has been preserved. Repair it before saving new recipes."
        );
        let mut custom = self.recipes[self.builtin_count..].to_vec();
        custom.push(recipe.clone());
        recipe::save_library(&self.library_path, &custom)?;
        self.recipes.push(recipe.clone());
        self.selected = self.recipes.len() - 1;
        self.current = recipe;
        self.changed();
        Ok(())
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").frame(egui::Frame::new().fill(PANEL).inner_margin(20)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| { ui.label(RichText::new("FUJI RECIPE STUDIO").size(20.).strong()); ui.label(RichText::new("YOUR PHOTOS. A DIFFERENT FEEL.").color(MUTED).size(10.)); });
                ui.add_space(28.);
                if ui.button("Open photos").on_hover_text("Ctrl+O · RAF, JPEG, PNG, TIFF").clicked() { self.open_files(); }
                if ui.button("+ Import recipe").clicked() { self.import.open = true; }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(self.full.is_some() && !self.exporting, egui::Button::new(RichText::new(if self.exporting { "Exporting…" } else { "Export image" }).color(BG).strong()).fill(ACCENT)).clicked() { self.export(ctx); }
                    if ui.button("?").clicked() { self.help_open = true; }
                    ui.label(RichText::new("APPROXIMATE FILM LOOKS").size(10.).color(ACCENT)).on_hover_text("Independent rendering. No camera connection. Results will differ from Fujifilm JPEGs.");
                });
            });
        });
    }

    fn library(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .exact_width(265.)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(18))
            .show(ctx, |ui| {
                ui.add_space(6.);
                ui.label(RichText::new("RECIPE LIBRARY").color(MUTED).size(11.));
                ui.add_space(6.);
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Find a recipe…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(12.);
                let mut choose = None;
                egui::ScrollArea::vertical()
                    .id_salt("recipe_scroll")
                    .show(ui, |ui| {
                        for (i, recipe) in self.recipes.iter().enumerate() {
                            if !recipe
                                .name
                                .to_lowercase()
                                .contains(&self.search.to_lowercase())
                            {
                                continue;
                            }
                            if i == self.builtin_count {
                                ui.add_space(12.);
                                ui.label(RichText::new("YOUR RECIPES").color(MUTED).size(11.));
                            }
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 78.),
                                Sense::click(),
                            );
                            let selected = self.selected == i;
                            ui.painter().rect_filled(
                                rect,
                                7.,
                                if selected {
                                    Color32::from_rgb(46, 59, 40)
                                } else if response.hovered() {
                                    CARD
                                } else {
                                    PANEL
                                },
                            );
                            if selected {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(rect.min, Vec2::new(3., rect.height())),
                                    2.,
                                    ACCENT,
                                );
                            }
                            let colors = palette(i);
                            for (j, color) in colors.iter().enumerate() {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        rect.min + Vec2::new(13. + j as f32 * 11., 13.),
                                        Vec2::new(11., 9.),
                                    ),
                                    1.,
                                    *color,
                                );
                            }
                            ui.painter().text(
                                rect.min + Vec2::new(13., 31.),
                                Align2::LEFT_TOP,
                                &recipe.name,
                                FontId::proportional(14.),
                                Color32::from_rgb(231, 236, 226),
                            );
                            ui.painter().text(
                                rect.min + Vec2::new(13., 53.),
                                Align2::LEFT_TOP,
                                &recipe.simulation,
                                FontId::proportional(11.),
                                MUTED,
                            );
                            if response.clicked() {
                                choose = Some(i);
                            }
                            ui.add_space(4.);
                        }
                    });
                if let Some(i) = choose {
                    self.selected = i;
                    self.current = self.recipes[i].clone();
                    self.changed();
                }
                ui.add_space(18.);
                ui.separator();
                ui.label(
                    RichText::new("Starter recipes by\nRitchie Roesch / Fuji X Weekly")
                        .color(MUTED)
                        .size(11.),
                );
                ui.hyperlink_to("Browse more recipes ↗", "https://fujixweekly.com/recipes/");
            });
    }

    fn settings(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("settings").exact_width(285.).frame(egui::Frame::new().fill(PANEL).inner_margin(18)).show(ctx, |ui| {
            ui.add_space(6.); ui.label(RichText::new("MAKE IT YOURS").color(MUTED).size(11.)); ui.add_space(10.);
            let mut changed = false;
            let mut reset = false;
            egui::ScrollArea::vertical().id_salt("settings_scroll").show(ui, |ui| {
                ui.label(RichText::new(&self.current.name).size(19.).strong());
                ui.add_space(10.);
                ui.label("Exposure");
                changed |= ui.add(egui::Slider::new(&mut self.exposure, -3.0..=3.0).step_by(0.05).suffix(" EV")).changed();
                ui.add_space(8.); ui.separator();
                changed |= select(ui, "Film simulation", &mut self.current.simulation, recipe::SIMULATIONS);
                changed |= select(ui, "White balance", &mut self.current.white_balance, recipe::WHITE_BALANCES);
                if self.current.white_balance == "Kelvin" { changed |= control(ui, "Temperature", &mut self.current.kelvin, 2000., 15000., 100.); }
                changed |= control(ui, "WB · Red", &mut self.current.wb_red, -9., 9., 1.);
                changed |= control(ui, "WB · Blue", &mut self.current.wb_blue, -9., 9., 1.);
                ui.separator();
                changed |= control(ui, "Highlights", &mut self.current.highlights, -2., 4., 0.5);
                changed |= control(ui, "Shadows", &mut self.current.shadows, -2., 4., 0.5);
                changed |= control(ui, "Color", &mut self.current.color, -4., 4., 1.);
                changed |= select(ui, "Dynamic range", &mut self.current.dynamic_range, &["Auto", "DR100", "DR200", "DR400"]);
                ui.collapsing("Texture & color effects", |ui| {
                    changed |= select(ui, "Grain", &mut self.current.grain, recipe::STRENGTHS);
                    changed |= select(ui, "Grain size", &mut self.current.grain_size, &["Small", "Large"]);
                    changed |= select(ui, "Color Chrome", &mut self.current.color_chrome, recipe::STRENGTHS);
                    changed |= select(ui, "Chrome Blue", &mut self.current.blue_chrome, recipe::STRENGTHS);
                    changed |= control(ui, "Sharpness", &mut self.current.sharpness, -4., 4., 1.);
                    changed |= control(ui, "Clarity", &mut self.current.clarity, -5., 5., 1.);
                    changed |= control(ui, "Noise reduction", &mut self.current.noise_reduction, -4., 4., 1.);
                    changed |= control(ui, "B&W · Warmth", &mut self.current.mono_wc, -18., 18., 1.);
                    changed |= control(ui, "B&W · Tint", &mut self.current.mono_mg, -18., 18., 1.);
                });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    if ui.button("Save as recipe").clicked() { self.save_name = format!("{} · custom", self.current.name); self.save_open = true; }
                    reset = ui.button("Reset").clicked();
                });
                ui.separator();
                if !self.current.author.is_empty() { ui.label(RichText::new(&self.current.author).color(MUTED).size(11.)); }
                if self.current.source.starts_with("https://") || self.current.source.starts_with("http://") { ui.hyperlink_to("Original recipe ↗", &self.current.source); }
                if !self.current.exposure_hint.is_empty() {
                    ui.label(RichText::new(format!("Shooting advice: {}", self.current.exposure_hint)).color(MUTED).size(11.));
                    ui.label(RichText::new("Adjust exposure for this photo; advice is not added automatically.").color(MUTED).size(11.));
                }
                ui.add_space(12.);
                ui.label(RichText::new("Independent approximations of Fujifilm settings. Fine detail and grain are best judged in the full-size export.").color(MUTED).size(11.));
            });
            if reset { self.current = self.recipes[self.selected].clone(); self.exposure = 0.; changed = true; }
            if changed { self.changed(); }
        });
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(20))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(
                                self.active_file
                                    .as_ref()
                                    .and_then(|p| p.file_name())
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "Your next favourite look".into()),
                            )
                            .size(16.)
                            .strong(),
                        );
                        if let Some(f) = &self.full {
                            ui.label(
                                RichText::new(format!(
                                    "{}  ·  {} × {}",
                                    f.model, f.width, f.height
                                ))
                                .color(MUTED)
                                .size(11.),
                            );
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.comparison, 2, "Compare");
                        ui.selectable_value(&mut self.comparison, 1, "Recipe");
                        ui.selectable_value(&mut self.comparison, 0, "Neutral");
                    });
                });
                ui.add_space(14.);
                let available = Vec2::new(
                    ui.available_width(),
                    (ui.available_height() - 80.).max(120.),
                );
                let (canvas, _) = ui.allocate_exact_size(available, Sense::hover());
                let texture = if self.comparison == 0 {
                    self.neutral.as_ref()
                } else {
                    self.rendered.as_ref().or(self.neutral.as_ref())
                };
                if let Some(texture) = texture {
                    let size = texture.size_vec2();
                    let ratio = (canvas.width() / size.x).min(canvas.height() / size.y);
                    let rect = Rect::from_center_size(canvas.center(), size * ratio);
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
                    ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
                    if self.comparison == 2 && self.rendered.is_some() {
                        if let Some(neutral) = &self.neutral {
                            let response =
                                ui.interact(rect, ui.id().with("split"), Sense::click_and_drag());
                            if response.dragged() || response.clicked() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    self.split =
                                        ((pos.x - rect.left()) / rect.width()).clamp(0.02, 0.98);
                                }
                            }
                            let x = rect.left() + rect.width() * self.split;
                            ui.painter()
                                .with_clip_rect(Rect::from_min_max(
                                    rect.min,
                                    Pos2::new(x, rect.bottom()),
                                ))
                                .image(neutral.id(), rect, uv, Color32::WHITE);
                            ui.painter().line_segment(
                                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                                Stroke::new(2_f32, Color32::WHITE),
                            );
                            ui.painter().circle_filled(
                                Pos2::new(x, rect.center().y),
                                15.,
                                Color32::from_white_alpha(230),
                            );
                            ui.painter().text(
                                Pos2::new(x, rect.center().y),
                                Align2::CENTER_CENTER,
                                "↔",
                                FontId::proportional(17.),
                                BG,
                            );
                            badge(ui, rect.min + Vec2::new(12., 12.), "NEUTRAL RAW");
                            badge(
                                ui,
                                Pos2::new(rect.right() - 94., rect.top() + 12.),
                                "RECIPE",
                            );
                        }
                    }
                } else {
                    let center = canvas.center();
                    let painter = ui.painter();
                    for (i, color) in palette(1).iter().enumerate() {
                        painter.rect_filled(
                            Rect::from_min_size(
                                center + Vec2::new(-60. + i as f32 * 24., -115.),
                                Vec2::new(24., 75.),
                            ),
                            2.,
                            *color,
                        );
                    }
                    painter.text(
                        center + Vec2::new(0., -5.),
                        Align2::CENTER_CENTER,
                        if self.loading {
                            "Developing your RAW photo…"
                        } else {
                            "Start with a photograph."
                        },
                        FontId::proportional(27.),
                        Color32::from_rgb(230, 235, 225),
                    );
                    painter.text(
                        center + Vec2::new(0., 35.),
                        Align2::CENTER_CENTER,
                        "Drop an X-T5 RAW here, or choose Open photos.",
                        FontId::proportional(14.),
                        MUTED,
                    );
                    painter.text(
                        center + Vec2::new(0., 62.),
                        Align2::CENTER_CENTER,
                        "Choose a recipe. Make it yours. Export.",
                        FontId::proportional(13.),
                        MUTED,
                    );
                }
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    if self.loading || self.render_running.is_some() || self.exporting {
                        ui.spinner();
                    }
                    ui.label(
                        RichText::new(if self.render_running.is_some() {
                            "Updating look…"
                        } else if self.full.is_some() {
                            "Full resolution ready to export"
                        } else {
                            "All photo processing stays on your PC"
                        })
                        .color(MUTED)
                        .size(11.),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.render_time != Duration::ZERO {
                            ui.label(
                                RichText::new(format!(
                                    "{:.0} ms preview",
                                    self.render_time.as_secs_f64() * 1000.
                                ))
                                .color(ACCENT)
                                .size(11.),
                            );
                        }
                    });
                });
                ui.add_space(8.);
                let mut choose = None;
                egui::ScrollArea::horizontal()
                    .id_salt("photo_strip")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for path in &self.files {
                                if ui
                                    .selectable_label(
                                        self.active_file.as_ref() == Some(path),
                                        path.file_name().unwrap_or_default().to_string_lossy(),
                                    )
                                    .clicked()
                                {
                                    choose = Some(path.clone());
                                }
                            }
                        });
                    });
                if let Some(path) = choose {
                    self.queue_load(path);
                }
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.import.open {
            let mut open = true;
            let mut fetch = false;
            let mut parse = false;
            let mut save = false;
            egui::Window::new("Import a recipe").open(&mut open).default_width(620.).collapsible(false).show(ctx, |ui| {
                ui.label("Paste a specific recipe URL, or copy the settings from a webpage.");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.import.url).hint_text("https://fujixweekly.com/…").desired_width(450.));
                    fetch = ui.add_enabled(!self.import.busy, egui::Button::new("Fetch recipe")).clicked();
                });
                if self.import.busy { ui.horizontal(|ui| { ui.spinner(); ui.label("Reading recipe page…"); }); }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.import.name).hint_text("Recipe name (optional)").desired_width(f32::INFINITY));
                ui.add(egui::TextEdit::multiline(&mut self.import.text).hint_text("Film Simulation: Classic Chrome\nWhite Balance: Daylight, +4 Red & -5 Blue\nHighlight: -1.5\nShadow: +0.5\nColor: +3\nGrain Effect: Strong, Small").desired_rows(10).desired_width(f32::INFINITY));
                parse = ui.button("Read pasted settings").clicked();
                if !self.import.error.is_empty() { ui.colored_label(Color32::from_rgb(245, 163, 132), &self.import.error); }
                if let Some(imported) = &self.import.preview {
                    ui.separator(); ui.label(RichText::new(&imported.recipe.name).strong());
                    ui.label(format!("{} · {} · R {:+} / B {:+}", imported.recipe.simulation, imported.recipe.white_balance, imported.recipe.wb_red, imported.recipe.wb_blue));
                    ui.label(format!("Highlights {:+} · Shadows {:+} · Color {:+} · Grain {}", imported.recipe.highlights, imported.recipe.shadows, imported.recipe.color, imported.recipe.grain));
                    for warning in imported.warnings.iter().take(5) { ui.label(RichText::new(warning).small().color(MUTED)); }
                    ui.label(RichText::new("Omitted settings use neutral defaults. You can edit every setting after import.").small().color(MUTED));
                    save = ui.button(RichText::new("Save to library & apply").color(ACCENT).strong()).clicked();
                }
            });
            self.import.open = open;
            if fetch {
                self.import.busy = true;
                self.import.error.clear();
                self.import.preview = None;
                let url = self.import.url.clone();
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = guarded(|| import::fetch_recipe(&url));
                    let _ = tx.send(Event::Imported(result));
                    ctx.request_repaint();
                });
            }
            if parse {
                match import::parse_text(&self.import.text, &self.import.name, &self.import.url) {
                    Ok(imported) => {
                        self.import.preview = Some(imported);
                        self.import.error.clear();
                    }
                    Err(e) => {
                        self.import.error = e.to_string();
                        self.import.preview = None;
                    }
                }
            }
            if save {
                if let Some(imported) = &self.import.preview {
                    match self.save_custom(imported.recipe.clone()) {
                        Ok(_) => {
                            self.import = ImportDialog::default();
                        }
                        Err(e) => self.import.error = e.to_string(),
                    }
                }
            }
        }
        if self.save_open {
            let mut open = true;
            let mut save = false;
            egui::Window::new("Save your recipe").open(&mut open).collapsible(false).show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(360.));
                ui.label(RichText::new("Exposure stays with the photo; recipe settings are saved to your library.").small().color(MUTED));
                save = ui.button("Save recipe").clicked();
            });
            self.save_open = open;
            if save {
                let mut recipe = self.current.clone();
                recipe.name = self.save_name.trim().into();
                match self.save_custom(recipe) {
                    Ok(_) => self.save_open = false,
                    Err(e) => self.error = e.to_string(),
                }
            }
        }
        if self.help_open {
            egui::Window::new("About your darkroom").open(&mut self.help_open).default_width(460.).show(ctx, |ui| {
                ui.label(RichText::new("Fuji Recipe Studio 0.1").size(21.).strong());
                ui.label("Open a RAW, choose a recipe, adjust exposure and export. Drag the divider in Compare to reveal the neutral RAW.");
                ui.separator();
                ui.label("The film looks are independent approximations. They do not use Fujifilm's proprietary rendering. Auto WB uses the white balance recorded in your photo. DR settings shape the tone curve; they cannot recover clipped highlights or change the original capture ISO.");
                ui.label("RAW input: Fujifilm X-Trans RAF, including X-T5. JPEG/PNG/TIFF inputs are treated as sRGB. Export: full-resolution JPEG or PNG, with a recipe JSON sidecar. Exports do not copy EXIF/GPS metadata.");
                ui.label("No camera connection, account or photo upload. Network access is used only when you request a recipe URL.");
                ui.label(format!("Your saved recipes: {}", self.library_path.display()));
                ui.label("Ctrl+O · Open photos     Ctrl+E · Export     Ctrl+I · Import recipe");
                ui.hyperlink_to("Recipe sources & project", "https://github.com/stefanradovanovic/fuji-recipe-studio");
            });
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.open_files();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::E)) {
            self.export(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::I)) {
            self.import.open = true;
        }
        self.toolbar(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(10))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.error.is_empty() {
                        ui.label(RichText::new(&self.status).small().color(MUTED));
                    } else {
                        ui.colored_label(Color32::from_rgb(245, 163, 132), &self.error);
                        if ui.small_button("Dismiss").clicked() {
                            self.error.clear();
                        }
                    }
                });
            });
        self.library(ctx);
        self.settings(ctx);
        self.canvas(ctx);
        self.dialogs(ctx);
        if self.capture_path.is_some()
            && self.rendered.is_some()
            && self.render_running.is_none()
            && self.render_due.is_none()
            && !self.capture_requested
        {
            self.capture_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(path) = &self.capture_path {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        path,
                        &bytes,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        self.error = e.to_string();
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

fn select(ui: &mut egui::Ui, label: &str, value: &mut String, choices: &[&str]) -> bool {
    let old = value.clone();
    ui.label(RichText::new(label).size(12.).color(MUTED));
    egui::ComboBox::from_id_salt(label)
        .selected_text(value.as_str())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for option in choices {
                ui.selectable_value(value, option.to_string(), *option);
            }
        });
    *value != old
}

fn control(ui: &mut egui::Ui, label: &str, value: &mut f32, lo: f32, hi: f32, step: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .range(lo..=hi)
                    .speed(step)
                    .max_decimals(if step < 1. { 1 } else { 0 }),
            )
            .changed()
        })
        .inner
    })
    .inner
}

fn palette(index: usize) -> [Color32; 5] {
    let values = match index {
        1 => [
            [64, 77, 57],
            [142, 139, 83],
            [208, 173, 89],
            [231, 205, 149],
            [111, 148, 143],
        ],
        2 => [
            [74, 87, 77],
            [158, 165, 132],
            [221, 198, 164],
            [230, 191, 166],
            [164, 183, 184],
        ],
        3 => [
            [34, 80, 91],
            [73, 134, 146],
            [146, 183, 180],
            [219, 191, 142],
            [179, 105, 69],
        ],
        4 => [
            [31, 33, 32],
            [79, 83, 80],
            [128, 133, 128],
            [186, 190, 183],
            [230, 233, 223],
        ],
        _ => [
            [70, 92, 78],
            [102, 124, 94],
            [151, 172, 121],
            [192, 210, 161],
            [215, 223, 199],
        ],
    };
    values.map(|v| Color32::from_rgb(v[0], v[1], v[2]))
}

fn badge(ui: &egui::Ui, pos: Pos2, label: &str) {
    ui.painter().rect_filled(
        Rect::from_min_size(pos, Vec2::new(83., 23.)),
        4.,
        Color32::from_black_alpha(150),
    );
    ui.painter().text(
        pos + Vec2::new(8., 6.),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(10.),
        Color32::WHITE,
    );
}

fn guarded<T>(f: impl FnOnce() -> anyhow::Result<T> + std::panic::UnwindSafe) -> anyhow::Result<T> {
    std::panic::catch_unwind(f).unwrap_or_else(|_| Err(anyhow::anyhow!("The image processor could not complete this operation. Your original file was not changed.")))
}
