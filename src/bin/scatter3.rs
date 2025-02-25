#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui::Color32;
use egui_plot::{CoordinatesFormatter, Corner, Plot, Points};
use hist3::data::InputSource;
use hist3::NUMERIC_REGEX;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread;

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Title
    #[arg(long, short, default_value = "Scatter Plot")]
    title: String,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone(); // Clone the title to avoid moving it

    let plot = ScatterApp::default();
    let data_ref = plot.data.clone();

    thread::spawn({
        let args = args; // Move args into the closure
        move || {
            let input = get_input_source(&args);
            process_input(input, &data_ref);
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0]) // Wider default window
            .with_min_inner_size([400.0, 300.0]), // Set minimum size
        ..Default::default()
    };
    eframe::run_native(&title, options, Box::new(|_| Ok(Box::new(plot))))
}

fn get_input_source(args: &Args) -> InputSource {
    if !atty::is(Stream::Stdin) {
        InputSource::Stdin
    } else {
        match &args.input {
            Some(file_name) => {
                if !Path::new(&file_name).exists() {
                    panic!("File does not exist");
                }
                InputSource::FileName(file_name.clone())
            }
            None => panic!("Input must either be piped in or provide a file"),
        }
    }
}

fn process_input(input: InputSource, data_ref: &Arc<RwLock<Vec<Vec<f64>>>>) {
    match input {
        InputSource::Stdin => {
            let reader = std::io::stdin();
            process_reader(reader.lock(), data_ref);
        }
        InputSource::FileName(file_name) => {
            let file = File::open(file_name).unwrap();
            let reader = BufReader::with_capacity(65536, file); // Use larger buffer for better performance
            process_reader(reader, data_ref);
        }
    };
}

fn process_reader<R: BufRead>(mut reader: R, data_ref: &Arc<RwLock<Vec<Vec<f64>>>>) {
    let mut first_line_size = None;
    let mut line = String::with_capacity(1024); // Pre-allocate line buffer
    let mut batch = Vec::with_capacity(1000); // Collect in batches to reduce lock contention

    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        let count = NUMERIC_REGEX.captures_iter(&line).count();
        let is_valid_line = first_line_size.map_or(true, |size| count == size);

        if is_valid_line {
            let floats = NUMERIC_REGEX
                .captures_iter(&line)
                .map(|cap| f64::from_str(&cap[0]).unwrap())
                .collect::<Vec<_>>();

            if first_line_size.is_none() {
                first_line_size = Some(floats.len());
            }

            batch.push(floats);

            // Write batch when it reaches capacity
            if batch.len() >= 1000 {
                if let Ok(mut data) = data_ref.write() {
                    data.append(&mut batch);
                }
                batch = Vec::with_capacity(1000);
            }
        }
        line.clear(); // Reuse the string buffer
    }

    // Add any remaining items
    if !batch.is_empty() {
        if let Ok(mut data) = data_ref.write() {
            data.append(&mut batch);
        }
    }
}

struct ScatterApp {
    data: Arc<RwLock<Vec<Vec<f64>>>>,
    x_col: usize,
    y_col: usize,
    color_col: Option<usize>,
    size_col: Option<usize>,
    color_cache: HashMap<usize, Vec<Color32>>,
    size_cache: HashMap<usize, Vec<f64>>,
    filters: Vec<(f64, f64, f64, f64)>,
    // Track statistics to avoid recomputing them
    statistics: HashMap<usize, (f64, f64)>, // (mean, std) for each column
    data_version: usize,                    // Incremented when data or filters change
    plot_data_cache: Option<(usize, Vec<([f64; 2], Option<f64>, Option<f64>)>)>,
}

impl Default for ScatterApp {
    fn default() -> Self {
        Self {
            data: Arc::new(RwLock::new(Vec::new())),
            x_col: 0,
            y_col: 1,
            color_col: None,
            size_col: None,
            color_cache: HashMap::new(),
            size_cache: HashMap::new(),
            filters: Vec::new(),
            statistics: HashMap::new(),
            data_version: 0,
            plot_data_cache: None,
        }
    }
}

impl eframe::App for ScatterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Only lock the data once per frame
        let column_count = {
            let data_read_guard = self.data.read().unwrap();
            if self.filters.len() != data_read_guard.first().map_or(0, |row| row.len()) {
                // Clone the data to avoid borrowing issues
                let data_clone = data_read_guard.clone();
                // Initialize filters only when necessary
                drop(data_read_guard); // Release the lock before mutating self
                self.init_filters(&data_clone);
                self.data_version += 1;
                data_clone.first().map_or(0, |row| row.len())
            } else {
                data_read_guard.first().map_or(0, |row| row.len())
            }
        };

        let mut data_changed = false;
        self.show_side_panel(ctx, column_count, &mut data_changed);

        if data_changed {
            self.data_version += 1;
        }

        // Generate color and size arrays only when needed
        if let Some(col) = self.color_col {
            if !self.color_cache.contains_key(&col) {
                self.generate_color_array();
            }
        }

        if let Some(col) = self.size_col {
            if !self.size_cache.contains_key(&col) {
                self.generate_size_array();
            }
        }

        // Use cached plot data if available and data hasn't changed
        let plot_data = if let Some((version, ref data)) = self.plot_data_cache {
            if version == self.data_version {
                data
            } else {
                let new_data = self.collect_plot_data();
                self.plot_data_cache = Some((self.data_version, new_data));
                &self.plot_data_cache.as_ref().unwrap().1
            }
        } else {
            let new_data = self.collect_plot_data();
            self.plot_data_cache = Some((self.data_version, new_data));
            &self.plot_data_cache.as_ref().unwrap().1
        };

        // Create longer-lived empty vectors for fallback cases
        let empty_color_vec = Vec::new();
        let empty_size_vec = Vec::new();

        let color_array = self
            .color_col
            .and_then(|col| self.color_cache.get(&col))
            .unwrap_or(&empty_color_vec);

        let size_array = self
            .size_col
            .and_then(|col| self.size_cache.get(&col))
            .unwrap_or(&empty_size_vec);

        self.show_central_panel(ctx, plot_data, color_array, size_array);

        // Request a redraw only if needed
        let data_size = self.data.read().unwrap().len();
        if data_size > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

impl ScatterApp {
    fn init_filters(&mut self, data: &[Vec<f64>]) {
        let columns = data.first().map_or(0, |row| row.len());

        self.filters.clear();
        for col in 0..columns {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;

            for row in data {
                if let Some(&val) = row.get(col) {
                    min = min.min(val);
                    max = max.max(val);
                }
            }

            self.filters.push((min, max, min, max));
        }
    }

    fn show_side_panel(
        &mut self,
        ctx: &egui::Context,
        column_count: usize,
        data_changed: &mut bool,
    ) {
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            let col_items: Vec<String> = (0..column_count).map(|i| i.to_string()).collect();

            let mut x_col = Some(self.x_col);
            let mut y_col = Some(self.y_col);
            let mut color_col = self.color_col;
            let mut size_col = self.size_col;

            self.create_combo_box(ui, "X Column", &mut x_col, &col_items);
            self.create_combo_box(ui, "Y Column", &mut y_col, &col_items);
            self.create_combo_box(ui, "Color Column", &mut color_col, &col_items);
            self.create_combo_box(ui, "Size Column", &mut size_col, &col_items);

            if color_col != self.color_col {
                self.color_col = color_col;
                if let Some(col) = self.color_col {
                    self.color_cache.remove(&col);
                }
                *data_changed = true;
            }

            if size_col != self.size_col {
                self.size_col = size_col;
                if let Some(col) = self.size_col {
                    self.size_cache.remove(&col);
                }
                *data_changed = true;
            }

            if x_col.unwrap_or(0) != self.x_col || y_col.unwrap_or(1) != self.y_col {
                self.x_col = x_col.unwrap_or(0);
                self.y_col = y_col.unwrap_or(1);
                *data_changed = true;
            }

            ui.separator();
            ui.heading("Filters");

            // Clone the data to avoid borrowing self for the entire duration
            let data_clone = if let Ok(data) = self.data.read() {
                Some(data.clone())
            } else {
                None
            };

            if let Some(data) = data_clone {
                // Calculate all statistics first
                let mut stats_to_update = Vec::new();

                for i in 0..self.filters.len() {
                    if !self.statistics.contains_key(&i) {
                        // Calculate statistics directly instead of calling the method
                        // to avoid mutable borrowing of self while data is borrowed
                        let filtered_data: Vec<f64> = data
                            .iter()
                            .filter(|row| {
                                row.iter().enumerate().all(|(j, val)| {
                                    if j < self.filters.len() {
                                        *val >= self.filters[j].2 && *val <= self.filters[j].3
                                    } else {
                                        true
                                    }
                                })
                            })
                            .filter_map(|row| row.get(i))
                            .cloned()
                            .collect();

                        let stats = if filtered_data.is_empty() {
                            (0.0, 0.0)
                        } else {
                            let mut sum = 0.0;
                            let mut sum_sq = 0.0;
                            let count = filtered_data.len() as f64;

                            for &val in &filtered_data {
                                sum += val;
                                sum_sq += val * val;
                            }

                            let mean = sum / count;
                            let variance = (sum_sq / count) - (mean * mean);
                            let std = variance.sqrt();

                            (mean, std)
                        };

                        stats_to_update.push((i, stats));
                    }
                }

                // Update statistics cache
                for (i, stats) in stats_to_update {
                    self.statistics.insert(i, stats);
                }

                // Now handle filters
                for (i, filter) in self.filters.iter_mut().enumerate() {
                    ui.strong(format!("Column {}", i));
                    let range = filter.0..=filter.1;

                    let old_min = filter.2;
                    let old_max = filter.3;

                    ui.add(egui::widgets::Slider::new(&mut filter.2, range.clone()).text("min"));
                    ui.add(egui::widgets::Slider::new(&mut filter.3, range).text("max"));

                    if old_min != filter.2 || old_max != filter.3 {
                        *data_changed = true;
                        self.statistics.remove(&i); // Invalidate statistics
                    }

                    // Use cached statistics
                    if let Some(&stats) = self.statistics.get(&i) {
                        let (mean, std) = stats;
                        ui.label(format!("Mean: {:.2}", mean));
                        ui.label(format!("Std: {:.2}", std));
                    }
                    ui.separator();
                }
            }
        });
    }

    fn create_combo_box(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        column: &mut Option<usize>,
        col_items: &[String],
    ) {
        egui::ComboBox::from_label(label)
            .selected_text(column.map_or("None".into(), |col| col.to_string()))
            .show_ui(ui, |ui| {
                ui.selectable_value(column, None, "None");
                for (i, item) in col_items.iter().enumerate() {
                    ui.selectable_value(column, Some(i), item);
                }
            });
    }

    fn generate_color_array(&mut self) {
        if let Some(col) = self.color_col {
            if let Ok(data) = self.data.read() {
                let mut values = Vec::new();
                let mut min_value = f64::INFINITY;
                let mut max_value = f64::NEG_INFINITY;

                // First pass - find min/max
                for row in data.iter() {
                    if let Some(&val) = row.get(col) {
                        min_value = min_value.min(val);
                        max_value = max_value.max(val);
                        values.push(val);
                    }
                }

                let range = max_value - min_value;

                // Second pass - create colors
                let colors: Vec<Color32> = if range == 0.0 {
                    vec![Color32::from_rgb(128, 64, 128); values.len()]
                } else {
                    values
                        .iter()
                        .map(|&val| {
                            let norm_value = (val - min_value) / range;
                            let r = (255.0 * norm_value).round() as u8;
                            let g = (norm_value * 128.0).round() as u8;
                            let b = (255.0 * (1.0 - norm_value)).round() as u8;
                            Color32::from_rgb(r, g, b)
                        })
                        .collect()
                };

                self.color_cache.insert(col, colors);
            }
        }
    }

    fn generate_size_array(&mut self) {
        if let Some(col) = self.size_col {
            if let Ok(data) = self.data.read() {
                let mut values = Vec::new();
                let mut min_value = f64::INFINITY;
                let mut max_value = f64::NEG_INFINITY;

                // First pass - find min/max
                for row in data.iter() {
                    if let Some(&val) = row.get(col) {
                        min_value = min_value.min(val);
                        max_value = max_value.max(val);
                        values.push(val);
                    }
                }

                let range = max_value - min_value;

                // Second pass - create sizes
                let sizes: Vec<f64> = if range == 0.0 {
                    vec![3.0; values.len()]
                } else {
                    values
                        .iter()
                        .map(|&val| {
                            let norm_value = (val - min_value) / range;
                            1.0 + 5.0 * norm_value
                        })
                        .collect()
                };

                self.size_cache.insert(col, sizes);
            }
        }
    }

    fn collect_plot_data(&self) -> Vec<([f64; 2], Option<f64>, Option<f64>)> {
        if let Ok(data) = self.data.read() {
            data.iter()
                .filter_map(|row| {
                    // Skip rows that don't pass the filter
                    for (i, val) in row.iter().enumerate() {
                        if i >= self.filters.len() {
                            break;
                        }
                        let filter = &self.filters[i];
                        if *val < filter.2 || *val > filter.3 {
                            return None;
                        }
                    }

                    if row.len() > self.x_col && row.len() > self.y_col {
                        let color = self.color_col.and_then(|c| row.get(c)).cloned();
                        let size = self.size_col.and_then(|s| row.get(s)).cloned();
                        Some(([row[self.x_col], row[self.y_col]], color, size))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    fn show_central_panel(
        &self,
        ctx: &egui::Context,
        plot_data: &[([f64; 2], Option<f64>, Option<f64>)],
        color_array: &[Color32],
        size_array: &[f64],
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .show_grid(true)
                .show_axes(true)
                .coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());

            plot.show(ui, |plot_ui| {
                // Group points by color and size for more efficient rendering
                // Use u32 for size instead of f32 to satisfy Eq+Hash requirements
                let mut point_groups: HashMap<(Color32, u32), Vec<[f64; 2]>> = HashMap::new();

                for (i, (pos, _, size_val)) in plot_data.iter().enumerate() {
                    let color = if !color_array.is_empty() && i < color_array.len() {
                        color_array[i]
                    } else {
                        Color32::GRAY
                    };

                    let size = size_val.map_or(2.0, |_| {
                        if !size_array.is_empty() && i < size_array.len() {
                            size_array[i]
                        } else {
                            2.0
                        }
                    });

                    // Convert size to u32 for hashing
                    let size_key = (size * 100.0) as u32;
                    let key = (color, size_key);
                    point_groups.entry(key).or_default().push(*pos);
                }

                // Render each group with a single Points object
                for ((color, size_key), positions) in point_groups {
                    // Convert size back to f32
                    let size = (size_key as f32) / 100.0;
                    let points = Points::new(positions).radius(size).color(color);
                    plot_ui.points(points);
                }
            });
        });
    }
}
