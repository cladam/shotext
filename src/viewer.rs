use eframe::egui;
use std::sync::Arc;

pub struct ShotViewer {
    image_uri: String,
    image_bytes: Arc<[u8]>,
    extracted_text: String,
    file_path: String,
}

impl ShotViewer {
    pub fn new(path: &str, text: String, image_bytes: Vec<u8>) -> Self {
        Self {
            image_uri: format!("bytes://{}", path),
            image_bytes: Arc::from(image_bytes),
            extracted_text: text,
            file_path: path.to_string(),
        }
    }

    /// Launch the viewer as a native desktop window.
    /// Blocks until the window is closed.
    pub fn launch(self) -> eframe::Result {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_title(format!("Shotext — {}", self.file_name())),
            ..Default::default()
        };

        eframe::run_native(
            "Shotext Viewer",
            options,
            Box::new(|cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                Ok(Box::new(self))
            }),
        )
    }

    fn file_name(&self) -> &str {
        std::path::Path::new(&self.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.file_path)
    }
}

impl eframe::App for ShotViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("📸 {}", self.file_name()));
            ui.separator();

            let panel_height = ui.available_height();

            ui.columns(2, |columns| {
                // ── Left: Screenshot ──
                columns[0].push_id("image_pane", |ui| {
                    ui.vertical_centered(|ui| {
                        ui.strong("Screenshot");
                        ui.separator();
                        egui::ScrollArea::both()
                            .id_salt("image_scroll")
                            .max_height(panel_height - 50.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Image::from_bytes(
                                        self.image_uri.clone(),
                                        self.image_bytes.clone(),
                                    )
                                    .max_width(ui.available_width())
                                    .shrink_to_fit(),
                                );
                            });
                    });
                });

                // ── Right: Extracted Text ──
                columns[1].push_id("text_pane", |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("OCR Extracted Text");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("📋 Copy to Clipboard").clicked() {
                                ui.ctx().copy_text(self.extracted_text.clone());
                            }
                        });
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("text_scroll")
                        .max_height(panel_height - 50.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.extracted_text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
            });
        });
    }
}