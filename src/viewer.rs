use eframe::egui;
use std::sync::Arc;

use crate::error::AppError;
use crate::{colours, db, search};

pub struct ShotViewer {
    image_uri: String,
    image_bytes: Arc<[u8]>,
    extracted_text: String,
    file_path: String,
    hash: String,
    tags: Vec<String>,
    tag_input: String,
    confirm_delete: bool,
    deleted: bool,
    db: sled::Db,
    tantivy_writer: tantivy::IndexWriter,
}

impl ShotViewer {
    pub fn new(
        path: &str,
        text: String,
        image_bytes: Vec<u8>,
        hash: String,
        tags: Vec<String>,
        db: sled::Db,
        index: &tantivy::Index,
    ) -> Result<Self, AppError> {
        let writer = search::writer(index).map_err(|e| AppError::Search(e.to_string()))?;
        Ok(Self {
            image_uri: format!("bytes://{}", path),
            image_bytes: Arc::from(image_bytes),
            extracted_text: text,
            file_path: path.to_string(),
            hash,
            tags,
            tag_input: String::new(),
            confirm_delete: false,
            deleted: false,
            db,
            tantivy_writer: writer,
        })
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

    fn perform_delete(&mut self) {
        if let Err(e) = search::delete_document(&mut self.tantivy_writer, &self.hash) {
            colours::warn(&format!("Failed to remove from search index: {e}"));
        }
        if let Err(e) = db::delete_record(&self.db, &self.hash) {
            colours::warn(&format!("Failed to remove from database: {e}"));
        }
        if let Err(e) = std::fs::remove_file(&self.file_path) {
            colours::warn(&format!("Failed to delete file {}: {e}", self.file_path));
        }
        self.deleted = true;
        self.confirm_delete = false;
    }
}

impl eframe::App for ShotViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Deleted state ──
            if self.deleted {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.label(egui::RichText::new("🗑").size(48.0));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Screenshot deleted.").size(16.0).weak());
                    });
                });
                return;
            }

            // ── Toolbar ──
            ui.horizontal(|ui| {
                ui.heading(format!("📸 {}", self.file_name()));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.confirm_delete {
                        ui.label(
                            egui::RichText::new("Delete permanently?")
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                        if ui
                            .button(
                                egui::RichText::new("Yes, delete")
                                    .color(egui::Color32::from_rgb(255, 80, 80)),
                            )
                            .clicked()
                        {
                            self.perform_delete();
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete = false;
                        }
                    } else if ui
                        .button("🗑 Delete")
                        .on_hover_text("Delete screenshot from index, database, and disk")
                        .clicked()
                    {
                        self.confirm_delete = true;
                    }
                });
            });
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

                // ── Right: Tags + Extracted Text ──
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
                            // ── Tags section ──
                            ui.add_space(4.0);
                            ui.strong("🏷 Tags");
                            ui.add_space(2.0);

                            let mut tag_to_remove: Option<String> = None;
                            ui.horizontal_wrapped(|ui| {
                                for tag in &self.tags {
                                    if ui
                                        .button(format!("{} ✕", tag))
                                        .on_hover_text("Click to remove tag")
                                        .clicked()
                                    {
                                        tag_to_remove = Some(tag.clone());
                                    }
                                }
                            });

                            if let Some(tag) = tag_to_remove {
                                if let Ok(Some(record)) = db::remove_tag(&self.db, &self.hash, &tag)
                                {
                                    self.tags = record.tags.clone();
                                    if let Err(e) = search::reindex_document(
                                        &mut self.tantivy_writer,
                                        &self.hash,
                                        &record,
                                    ) {
                                        colours::warn(&format!("Failed to reindex: {e}"));
                                    }
                                }
                            }

                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.tag_input)
                                        .hint_text("New tag…")
                                        .desired_width(ui.available_width() - 50.0),
                                );
                                let enter_pressed = response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                                if (ui.button("+").clicked() || enter_pressed)
                                    && !self.tag_input.trim().is_empty()
                                {
                                    let new_tag = self.tag_input.trim().to_string();
                                    self.tag_input.clear();
                                    if let Ok(Some(record)) =
                                        db::add_tag(&self.db, &self.hash, &new_tag)
                                    {
                                        self.tags = record.tags.clone();
                                        if let Err(e) = search::reindex_document(
                                            &mut self.tantivy_writer,
                                            &self.hash,
                                            &record,
                                        ) {
                                            colours::warn(&format!("Failed to reindex: {e}"));
                                        }
                                    }
                                }
                            });

                            ui.separator();

                            // ── Extracted text ──
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
