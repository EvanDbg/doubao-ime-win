//! Settings window implemented with eframe and egui.

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use eframe::egui::{
        self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
        RichText, Rounding, Sense, Stroke, Vec2, Visuals,
    };
    use eframe::{NativeOptions, Storage};
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    use crate::data::{PunctuationMode, RuntimeSettings};

    struct SettingsApp {
        settings: Arc<RuntimeSettings>,
        mode: PunctuationMode,
        saved_at: Option<std::time::Instant>,
    }

    impl SettingsApp {
        fn new(settings: Arc<RuntimeSettings>) -> Self {
            let config = settings.get();
            let mode = if config.asr.caption_punctuation && config.asr.sentence_punctuation {
                config.asr.punctuation_mode
            } else {
                PunctuationMode::from_legacy(
                    config.asr.caption_punctuation,
                    config.asr.sentence_punctuation,
                )
            };
            Self {
                settings,
                mode,
                saved_at: None,
            }
        }

        fn select_mode(&mut self, mode: PunctuationMode) {
            if mode == self.mode {
                return;
            }
            self.mode = mode;
            let mut config = self.settings.get();
            config.asr.punctuation_mode = mode;
            config.asr.caption_punctuation = mode != PunctuationMode::Full;
            config.asr.sentence_punctuation = mode != PunctuationMode::Space;
            match self.settings.update(config) {
                Ok(()) => {
                    self.saved_at = Some(std::time::Instant::now());
                    tracing::info!("Punctuation mode saved: {:?}", mode);
                }
                Err(error) => {
                    tracing::error!("Failed to save punctuation mode: {:?}", error);
                }
            }
        }
    }

    impl eframe::App for SettingsApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            let dark = system_prefers_dark();
            ctx.set_visuals(theme_visuals(dark));

            egui::CentralPanel::default()
                .frame(
                    Frame::none()
                        .fill(surface_color(dark))
                        .inner_margin(Margin::symmetric(24.0, 20.0)),
                )
                .show(ctx, |ui| {
                    ui.heading(RichText::new("标点设置").size(22.0).strong());
                    ui.add_space(18.0);

                    Frame::none()
                        .fill(card_color(dark))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::symmetric(18.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            let smart = option_row(
                                ui,
                                self.mode == PunctuationMode::Smart,
                                "智能添加标点",
                                "自动为语音内容补充标点",
                                dark,
                            );
                            if smart.clicked() {
                                self.select_mode(PunctuationMode::Smart);
                            }
                            ui.separator();
                            let space = option_row(
                                ui,
                                self.mode == PunctuationMode::Space,
                                "空格代替标点",
                                "以空格分隔短句，不插入标点符号",
                                dark,
                            );
                            if space.clicked() {
                                self.select_mode(PunctuationMode::Space);
                            }
                            ui.separator();
                            let full = option_row(
                                ui,
                                self.mode == PunctuationMode::Full,
                                "添加完整标点",
                                "输出句号、逗号等完整标点符号",
                                dark,
                            );
                            if full.clicked() {
                                self.select_mode(PunctuationMode::Full);
                            }
                        });

                    ui.add_space(14.0);
                    match self.saved_at {
                        Some(saved_at) if saved_at.elapsed().as_secs_f32() < 1.8 => {
                            ui.label(RichText::new("已保存").size(13.0).color(accent_color(dark)));
                        }
                        _ => {
                            ui.label(
                                RichText::new("选择后立即保存")
                                    .size(13.0)
                                    .color(muted_color(dark)),
                            );
                        }
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.add_space(2.0);
                        let exit_button = ui.add_sized(
                            [ui.available_width(), 38.0],
                            egui::Button::new(RichText::new("退出本输入法").size(15.0)),
                        );
                        if exit_button.clicked() {
                            tracing::info!("Exit requested from settings");
                            EXIT_REQUESTED.store(true, Ordering::SeqCst);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        }

        fn save(&mut self, _storage: &mut dyn Storage) {}
    }

    fn option_row(
        ui: &mut egui::Ui,
        selected: bool,
        title: &str,
        subtitle: &str,
        dark: bool,
    ) -> egui::Response {
        let size = Vec2::new(ui.available_width(), 50.0);
        let response = ui.allocate_response(size, Sense::click());
        let painter = ui.painter();
        let rect = response.rect;

        painter.text(
            rect.left_center() + Vec2::new(2.0, -8.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(16.0),
            text_color(dark),
        );
        painter.text(
            rect.left_center() + Vec2::new(2.0, 11.0),
            Align2::LEFT_CENTER,
            subtitle,
            FontId::proportional(12.0),
            muted_color(dark),
        );

        let check_center = rect.right_center() - Vec2::new(14.0, 0.0);
        if selected {
            painter.circle_filled(check_center, 10.0_f32, accent_color(dark));
            painter.line_segment(
                [check_center + Vec2::new(-5.0, 0.0), check_center],
                Stroke::new(2.0_f32, Color32::WHITE),
            );
            painter.line_segment(
                [check_center, check_center + Vec2::new(5.0, 5.0)],
                Stroke::new(2.0_f32, Color32::WHITE),
            );
        }

        response
    }

    fn theme_visuals(dark: bool) -> Visuals {
        let mut visuals = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.window_stroke = Stroke::NONE;
        visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent_color(dark));
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent_color(dark));
        visuals.selection.stroke = Stroke::new(1.0_f32, accent_color(dark));
        visuals
    }

    fn surface_color(dark: bool) -> Color32 {
        if dark {
            Color32::from_rgb(8, 8, 8)
        } else {
            Color32::from_rgb(246, 247, 249)
        }
    }

    fn card_color(dark: bool) -> Color32 {
        if dark {
            Color32::from_rgb(24, 24, 24)
        } else {
            Color32::WHITE
        }
    }

    fn text_color(dark: bool) -> Color32 {
        if dark {
            Color32::WHITE
        } else {
            Color32::from_rgb(23, 23, 23)
        }
    }

    fn muted_color(dark: bool) -> Color32 {
        if dark {
            Color32::from_rgb(138, 138, 138)
        } else {
            Color32::from_rgb(110, 110, 110)
        }
    }

    fn accent_color(dark: bool) -> Color32 {
        if dark {
            Color32::from_rgb(52, 199, 89)
        } else {
            Color32::from_rgb(22, 163, 74)
        }
    }

    fn system_prefers_dark() -> bool {
        unsafe {
            let mut value = [0u16; 32];
            let mut size = u32::try_from(value.len() * 2).unwrap_or(0);
            let status = RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_SZ,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut size),
            );
            status.is_ok()
                && String::from_utf16_lossy(&value)
                    .trim_end_matches(char::from(0))
                    .trim()
                    == "0"
        }
    }

    fn load_fonts(ctx: &egui::Context) {
        let font_data = [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
        ]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .expect("No usable CJK font found");
        let mut fonts = FontDefinitions::default();
        fonts
            .font_data
            .insert("system_cjk".to_owned(), FontData::from_owned(font_data));
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "system_cjk".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push("system_cjk".to_owned());
        ctx.set_fonts(fonts);
    }

    static SETTINGS_OPEN: AtomicUsize = AtomicUsize::new(0);
    static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

    pub fn exit_requested() -> bool {
        EXIT_REQUESTED.load(Ordering::SeqCst)
    }

    pub fn open(settings: Arc<RuntimeSettings>) {
        EXIT_REQUESTED.store(false, Ordering::SeqCst);
        if SETTINGS_OPEN.load(Ordering::SeqCst) != 0 {
            return;
        }
        SETTINGS_OPEN.store(1, Ordering::SeqCst);

        std::thread::spawn(move || {
            let options = NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_title("标点设置")
                    .with_inner_size([420.0, 330.0])
                    .with_resizable(false),
                vsync: true,
                ..Default::default()
            };
            let result = eframe::run_native(
                "标点设置",
                options,
                Box::new(move |creation_context| {
                    load_fonts(&creation_context.egui_ctx);
                    Box::new(SettingsApp::new(settings))
                }),
            );
            SETTINGS_OPEN.store(0, Ordering::SeqCst);
            if let Err(error) = result {
                tracing::error!("Settings window closed with error: {:?}", error);
            }
        });
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::{exit_requested, open};

#[cfg(not(target_os = "windows"))]
pub fn open(_settings: Arc<RuntimeSettings>) {}

#[cfg(not(target_os = "windows"))]
pub fn exit_requested() -> bool {
    false
}
