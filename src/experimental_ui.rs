use eframe::egui;
use std::sync::Arc;

use crate::error::AppError;
use crate::{colours, db, ocr, search};

struct DashboardEntry {
    hash: String,
    path: String,
    content: String,
    created_at: String,
    tags: Vec<String>,
}

/// The currently loaded image for the detail view.
struct LoadedImage {
    /// Index in `all_entries` this image belongs to.
    entry_index: usize,
    uri: String,
    bytes: Arc<[u8]>,
}

struct ShotextDashboard {
    // Data
    all_entries: Vec<DashboardEntry>,
    filtered_indices: Vec<usize>,
    tantivy_index: tantivy::Index,
    tantivy_writer: tantivy::IndexWriter,
    db: sled::Db,

    // UI state
    search_query: String,
    prev_query: String,
    selected_index: Option<usize>, // index into filtered_indices
    loaded_image: Option<LoadedImage>,
    text_panel_open: bool,
    focus_search: bool,
    confirm_delete: Option<usize>, // entry index pending deletion
    tag_input: String,             // text field for adding new tags
}

impl ShotextDashboard {
    fn new(
        records: Vec<search::SearchResult>,
        index: tantivy::Index,
        db: sled::Db,
    ) -> Result<Self, AppError> {
        let writer = search::writer(&index).map_err(|e| AppError::Search(e.to_string()))?;

        let entries: Vec<DashboardEntry> = records
            .into_iter()
            .map(|r| DashboardEntry {
                hash: r.hash,
                path: r.path,
                content: r.content,
                created_at: r.created_at,
                tags: r.tags,
            })
            .collect();
        let mut records = entries;
        // Sort the list with the newest first
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let filtered_indices: Vec<usize> = (0..records.len()).collect();
        let selected = if records.is_empty() { None } else { Some(0) };

        Ok(Self {
            all_entries: records,
            filtered_indices,
            tantivy_index: index,
            tantivy_writer: writer,
            db,
            search_query: String::new(),
            prev_query: String::new(),
            selected_index: selected,
            loaded_image: None,
            text_panel_open: true,
            focus_search: false,
            confirm_delete: None,
            tag_input: String::new(),
        })
    }

    /// Run Tantivy search or show all if query is empty.
    fn refresh_filter(&mut self) {
        if self.search_query.trim().is_empty() {
            // Show everything
            self.filtered_indices = (0..self.all_entries.len()).collect();
        } else {
            // Use Tantivy full-text search
            match search::query(&self.tantivy_index, &self.search_query, 100) {
                Ok(results) => {
                    // Map search results back to entry indices via hash
                    self.filtered_indices = results
                        .iter()
                        .filter_map(|sr| self.all_entries.iter().position(|e| e.hash == sr.hash))
                        .collect();
                }
                Err(_) => {
                    // On parse error, fall back to simple substring match
                    let q = self.search_query.to_lowercase();
                    self.filtered_indices = self
                        .all_entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            e.content.to_lowercase().contains(&q)
                                || e.path.to_lowercase().contains(&q)
                        })
                        .map(|(i, _)| i)
                        .collect();
                }
            }
        }

        // Reset selection
        self.selected_index = if self.filtered_indices.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Ensure the image for the currently selected entry is loaded.
    fn ensure_image_loaded(&mut self) {
        let entry_idx = match self.selected_index {
            Some(sel) => self.filtered_indices.get(sel).copied(),
            None => None,
        };

        let entry_idx = match entry_idx {
            Some(i) => i,
            None => {
                self.loaded_image = None;
                return;
            }
        };

        // Already loaded?
        if let Some(ref img) = self.loaded_image {
            if img.entry_index == entry_idx {
                return;
            }
        }

        // Load from disk
        let path = &self.all_entries[entry_idx].path;
        match std::fs::read(path) {
            Ok(bytes) => {
                self.loaded_image = Some(LoadedImage {
                    entry_index: entry_idx,
                    uri: format!("bytes://{}", path),
                    bytes: Arc::from(bytes),
                });
            }
            Err(_) => {
                self.loaded_image = None;
            }
        }
    }

    /// Helper: get the real entry index for the current selection.
    fn selected_entry_idx(&self) -> Option<usize> {
        self.selected_index
            .and_then(|sel| self.filtered_indices.get(sel).copied())
    }

    /// Delete an entry from the search index, database, and disk.
    fn delete_entry(&mut self, entry_idx: usize) {
        let entry = &self.all_entries[entry_idx];

        // 1. Remove from Tantivy search index
        if let Err(e) = search::delete_document(&mut self.tantivy_writer, &entry.hash) {
            colours::warn(&format!("Failed to remove from search index: {e}"));
        }

        // 2. Remove from sled database
        if let Err(e) = db::delete_record(&self.db, &entry.hash) {
            colours::warn(&format!("Failed to remove from database: {e}"));
        }

        // 3. Delete file from disk
        if let Err(e) = std::fs::remove_file(&entry.path) {
            colours::warn(&format!("Failed to delete file {}: {e}", entry.path));
        }

        // 4. Remove from in-memory state
        self.all_entries.remove(entry_idx);

        // Rebuild filtered_indices (old indices are now stale)
        self.filtered_indices.retain(|&i| i != entry_idx);
        for idx in &mut self.filtered_indices {
            if *idx > entry_idx {
                *idx -= 1;
            }
        }

        // Adjust selection
        if self.filtered_indices.is_empty() {
            self.selected_index = None;
            self.loaded_image = None;
        } else if let Some(sel) = self.selected_index {
            if sel >= self.filtered_indices.len() {
                self.selected_index = Some(self.filtered_indices.len() - 1);
            }
        }

        self.confirm_delete = None;
    }
}

impl eframe::App for ShotextDashboard {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Keyboard shortcut: ⌘F / Ctrl+F to focus search ──
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            self.focus_search = true;
        }

        // ── Arrow key navigation in the sidebar list ──
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            if let Some(sel) = self.selected_index {
                if sel + 1 < self.filtered_indices.len() {
                    self.selected_index = Some(sel + 1);
                }
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            if let Some(sel) = self.selected_index {
                if sel > 0 {
                    self.selected_index = Some(sel - 1);
                }
            }
        }

        // Lazy-load image for whatever is selected
        self.ensure_image_loaded();

        // Left panel
        egui::SidePanel::left("navigation_sidebar")
            .resizable(true)
            .default_width(320.0)
            .min_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);

                // ── Search bar ──
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label("🔍");
                    let search_field = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search…  (⌘F)")
                            .desired_width(ui.available_width()),
                    );
                    if self.focus_search {
                        search_field.request_focus();
                        self.focus_search = false;
                    }
                    if search_field.changed() && self.search_query != self.prev_query {
                        self.prev_query = self.search_query.clone();
                        self.refresh_filter();
                    }
                });

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} of {} screenshots",
                        self.filtered_indices.len(),
                        self.all_entries.len()
                    ))
                    .size(11.0)
                    .weak(),
                );
                ui.separator();

                // ── Results list (virtualised scroll — only visible rows rendered) ──
                let row_height = 58.0;
                let num_rows = self.filtered_indices.len();

                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, num_rows, |ui, row_range| {
                        for row_idx in row_range {
                            let entry_idx = self.filtered_indices[row_idx];
                            let entry = &self.all_entries[entry_idx];
                            // let mut records = entry;
                            // records.sort_by(|a, b| a.created_at.cmp(&b.created_at));

                            let is_selected = self.selected_index == Some(row_idx);

                            let filename = std::path::Path::new(&entry.path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();

                            let snippet = ocr::truncate(&entry.content, 60).replace('\n', " ");

                            let response = ui.push_id(row_idx, |ui| {
                                let frame = egui::Frame::NONE
                                    .fill(if is_selected {
                                        ui.visuals().selection.bg_fill
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .corner_radius(6.0)
                                    .inner_margin(egui::Margin::same(6));

                                frame.show(ui, |ui| {
                                    ui.set_width(ui.available_width());

                                    ui.label(
                                        egui::RichText::new(format!("📄 {}", filename))
                                            .strong()
                                            .size(13.0),
                                    );

                                    ui.label(
                                        egui::RichText::new(&entry.created_at).size(10.0).weak(),
                                    );

                                    if !snippet.is_empty() {
                                        ui.label(egui::RichText::new(&snippet).size(10.5).weak());
                                    }
                                })
                            });

                            if response.response.interact(egui::Sense::click()).clicked() {
                                self.selected_index = Some(row_idx);
                            }
                        }
                    });
            });

        // Right panel
        if self.text_panel_open {
            if let Some(entry_idx) = self.selected_entry_idx() {
                egui::SidePanel::right("text_drawer")
                    .resizable(true)
                    .default_width(350.0)
                    .min_width(200.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("Extracted Text");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("✕").on_hover_text("Close text panel").clicked()
                                    {
                                        self.text_panel_open = false;
                                    }
                                    if ui.button("📋 Copy").clicked() {
                                        ui.ctx()
                                            .copy_text(self.all_entries[entry_idx].content.clone());
                                    }
                                },
                            );
                        });
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("text_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // ── Tags section ──
                                ui.add_space(4.0);
                                ui.strong("🏷 Tags");
                                ui.add_space(2.0);

                                // Show existing tags as removable chips
                                let mut tag_to_remove: Option<String> = None;
                                ui.horizontal_wrapped(|ui| {
                                    for tag in &self.all_entries[entry_idx].tags {
                                        let btn = ui.button(format!("{} ✕", tag));
                                        if btn.on_hover_text("Click to remove tag").clicked() {
                                            tag_to_remove = Some(tag.clone());
                                        }
                                    }
                                });

                                // Remove tag if clicked
                                if let Some(tag) = tag_to_remove {
                                    let hash = self.all_entries[entry_idx].hash.clone();
                                    if let Ok(Some(record)) = db::remove_tag(&self.db, &hash, &tag)
                                    {
                                        self.all_entries[entry_idx].tags = record.tags.clone();
                                        if let Err(e) = search::reindex_document(
                                            &mut self.tantivy_writer,
                                            &hash,
                                            &record,
                                        ) {
                                            colours::warn(&format!("Failed to reindex: {e}"));
                                        }
                                    }
                                }

                                // Add new tag input
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
                                        let hash = self.all_entries[entry_idx].hash.clone();
                                        if let Ok(Some(record)) =
                                            db::add_tag(&self.db, &hash, &new_tag)
                                        {
                                            self.all_entries[entry_idx].tags = record.tags.clone();
                                            if let Err(e) = search::reindex_document(
                                                &mut self.tantivy_writer,
                                                &hash,
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
                                    egui::TextEdit::multiline(
                                        &mut self.all_entries[entry_idx].content,
                                    )
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                                );
                            });
                    });
            }
        }

        // Center - the image itself
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(entry_idx) = self.selected_entry_idx() {
                let filename = std::path::Path::new(&self.all_entries[entry_idx].path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let created_at = self.all_entries[entry_idx].created_at.clone();
                let path = self.all_entries[entry_idx].path.clone();

                // Toolbar
                ui.horizontal(|ui| {
                    ui.heading(format!("📸 {}", filename));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Delete button
                        if self.confirm_delete == Some(entry_idx) {
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
                                self.delete_entry(entry_idx);
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_delete = None;
                            }
                        } else if ui
                            .button("🗑 Delete")
                            .on_hover_text("Delete screenshot from index, database, and disk")
                            .clicked()
                        {
                            self.confirm_delete = Some(entry_idx);
                        }

                        ui.add_space(8.0);

                        if !self.text_panel_open
                            && ui
                                .button("📝 Show Text")
                                .on_hover_text("Open extracted text panel")
                                .clicked()
                        {
                            self.text_panel_open = true;
                        }
                    });
                });

                ui.label(
                    egui::RichText::new(format!("{}  •  {}", created_at, path))
                        .size(11.0)
                        .weak(),
                );
                ui.separator();

                // Image
                if let Some(ref img) = self.loaded_image {
                    if img.entry_index == entry_idx {
                        egui::ScrollArea::both()
                            .id_salt("image_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Image::from_bytes(img.uri.clone(), img.bytes.clone())
                                        .max_width(ui.available_width())
                                        .shrink_to_fit(),
                                );
                            });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("⚠ Could not load image from disk");
                    });
                }
            } else {
                // Empty state
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.label(egui::RichText::new("📷").size(48.0).weak());
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Select a screenshot to view details")
                                .size(16.0)
                                .weak(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "Use the search bar or click an item in the sidebar",
                            )
                            .size(12.0)
                            .weak(),
                        );
                    });
                });
            }
        });
    }
}

/// Build the dashboard from sled records and launch the native window.
pub fn launch_dashboard(
    records: Vec<search::SearchResult>,
    index: tantivy::Index,
    db: sled::Db,
) -> Result<(), AppError> {
    let dashboard = ShotextDashboard::new(records, index, db)?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Shotext — Dashboard"),
        ..Default::default()
    };

    eframe::run_native(
        "Shotext Dashboard",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(dashboard))
        }),
    )
    .map_err(|e| AppError::GuiError(e.to_string()))
}
