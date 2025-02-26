#!/usr/bin/env rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui::Color32;
use egui_plot::{Bar, BarChart, CoordinatesFormatter, Corner, Legend, Plot, Points, VLine};
use hist3::data::InputSource;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;

mod colors {
    use eframe::egui::Color32;

    pub const PERCENTILE_25_COLOR: Color32 = Color32::from_rgb(77, 77, 255);
    pub const PERCENTILE_50_COLOR: Color32 = Color32::from_rgb(77, 255, 77);
    pub const PERCENTILE_75_COLOR: Color32 = Color32::from_rgb(255, 77, 77);
    pub const DEFAULT_BAR_COLOR: Color32 = Color32::from_rgb(75, 75, 75);
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Title
    #[arg(long, short, default_value = "Data Viewer")]
    title: String,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone();

    let app = MainApp::default();
    let data_ref = app.data.clone();

    thread::spawn(move || {
        let input = get_input_source(&args);
        process_input(input, &data_ref);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 600.0]) // Wider default window
            .with_min_inner_size([300.0, 300.0]), // Set minimum size
        ..Default::default()
    };
    eframe::run_native(title.as_str(), options, Box::new(|_| Ok(Box::new(app))))
}

fn get_input_source(args: &Args) -> InputSource {
    if !atty::is(Stream::Stdin) {
        InputSource::Stdin
    } else {
        let file_name = args
            .input
            .clone()
            .expect("Input must either be piped in or provide a file");
        if !Path::new(&file_name).exists() {
            panic!("File does not exist");
        }
        InputSource::FileName(file_name)
    }
}

fn process_input(input: InputSource, data_ref: &Arc<Mutex<Vec<Vec<f64>>>>) {
    match input {
        InputSource::Stdin => {
            let reader = std::io::stdin();
            process_reader(reader.lock(), data_ref);
        }
        InputSource::FileName(file_name) => {
            let file = File::open(file_name).unwrap();
            let reader = io::BufReader::new(file);
            process_reader(reader, data_ref);
        }
    };
}

fn process_reader<R: BufRead>(reader: R, data_ref: &Arc<Mutex<Vec<Vec<f64>>>>) {
    let mut first_line_size = None;
    let mut batch = Vec::new();
    const BATCH_SIZE: usize = 1000;

    for line in reader.lines() {
        if let Ok(line) = line {
            // Extract all numeric patterns that could be valid numbers
            let mut values = Vec::new();
            let mut start_idx = None;

            // Scan the line character by character to identify number patterns
            for (i, c) in line.char_indices() {
                let is_num_char =
                    c.is_numeric() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E';

                if is_num_char && start_idx.is_none() {
                    // Start of a new number
                    start_idx = Some(i);
                } else if !is_num_char && start_idx.is_some() {
                    // End of a number - extract the substring
                    let start = start_idx.unwrap();
                    values.push(&line[start..i]);
                    start_idx = None;
                }
            }

            // Handle case where the line ends with a number
            if let Some(start) = start_idx {
                values.push(&line[start..]);
            }

            // Parse all extracted strings into numbers, filtering out failures
            let floats = values
                .into_iter()
                .filter_map(|s| f64::from_str(s).ok())
                .collect::<Vec<_>>();

            // Check if the number of values matches the size of the first line
            // Also ensure we actually parsed some numbers
            if !floats.is_empty() {
                if first_line_size.is_none() {
                    first_line_size = Some(floats.len());
                    batch.push(floats);
                } else if floats.len() == first_line_size.unwrap() {
                    batch.push(floats);
                }

                // Only lock the mutex when we have a full batch
                if batch.len() >= BATCH_SIZE {
                    let mut data = data_ref.lock().unwrap();
                    data.extend(batch.drain(..));
                }
            }
        }
    }

    // Don't forget any remaining rows
    if !batch.is_empty() {
        let mut data = data_ref.lock().unwrap();
        data.extend(batch);
    }
}

struct MainApp {
    data: Arc<Mutex<Vec<Vec<f64>>>>,
    filters: Vec<(f64, f64, f64, f64)>,
    scatter_plots: Vec<(bool, Arc<Mutex<ScatterSettings>>)>, // (is_open, settings)
    histograms: Vec<(bool, Arc<Mutex<HistogramSettings>>)>,  // (is_open, settings)
    cached_column_labels: Vec<String>,
    _data_version: usize, // Used to track when data has changed
}

#[derive(Clone)]
struct ScatterSettings {
    x_col: usize,
    y_col: usize,
    color_col: Option<usize>,
    size_col: Option<usize>,
}

#[derive(Clone)]
struct HistogramSettings {
    column: usize,
    bins: usize,
    cached_stats: Option<(f64, f64, f64)>, // mean, variance, stddev
    last_data_version: usize,              // To detect when recalculation is needed
}

impl Default for ScatterSettings {
    fn default() -> Self {
        Self {
            x_col: 0,
            y_col: 1,
            color_col: None,
            size_col: None,
        }
    }
}

impl Default for HistogramSettings {
    fn default() -> Self {
        Self {
            column: 0,
            bins: 20,
            cached_stats: None,
            last_data_version: 0,
        }
    }
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            filters: Vec::new(),
            scatter_plots: Vec::new(),
            histograms: Vec::new(),
            cached_column_labels: Vec::new(),
            _data_version: 0,
        }
    }
}

impl eframe::App for MainApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if data has changed
        {
            let data = self.data.lock().unwrap();
            let column_count = data.first().map_or(0, |row| row.len());
            let _new_rows = data.len();

            // Update column labels if needed
            if self.cached_column_labels.len() != column_count && column_count > 0 {
                self.cached_column_labels = (0..column_count).map(|i| i.to_string()).collect();
            }

            // Initialize filters if needed or if data has changed
            if self.filters.len() != column_count {
                self._data_version += 1;
                self.filters.clear();

                if column_count > 0 {
                    // Calculate all column min/max in a single pass to improve efficiency
                    let mut mins = vec![f64::INFINITY; column_count];
                    let mut maxs = vec![f64::NEG_INFINITY; column_count];

                    for row in data.iter() {
                        for (i, &val) in row.iter().enumerate().take(column_count) {
                            mins[i] = mins[i].min(val);
                            maxs[i] = maxs[i].max(val);
                        }
                    }

                    for i in 0..column_count {
                        self.filters.push((mins[i], maxs[i], mins[i], maxs[i]));
                    }
                }
            }
        }

        self.show_main_panel(ctx);

        // Handle scatter plot windows
        let mut scatter_to_remove = Vec::new();

        // First handle existing scatter plot windows
        for (i, (is_open, settings)) in self.scatter_plots.iter_mut().enumerate() {
            if !*is_open {
                scatter_to_remove.push(i);
                continue;
            }

            // Create a unique ID for each window
            let viewport_id = egui::ViewportId::from_hash_of(&format!("scatter_plot_{}", i));
            let window_title = format!("Scatter Plot {}", i + 1);

            // Clone the shared references for the window
            let settings_arc = settings.clone();
            let data_arc = self.data.clone();
            let filters_ref = &self.filters; // Use reference instead of cloning
            let _data_version = self._data_version;

            // Create a mutable reference to is_open to track window state
            let is_open_ref = is_open;

            // Show the window using immediate viewport
            ctx.show_viewport_immediate(
                viewport_id,
                egui::ViewportBuilder::default()
                    .with_title(window_title)
                    .with_inner_size([900.0, 700.0]),
                move |ctx, _| {
                    // Check if the window's close button was clicked
                    ctx.input(|i| {
                        if i.viewport().close_requested() {
                            *is_open_ref = false;
                        }
                    });

                    // This closure runs every frame for the viewport
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_space(10.0);
                        // Get filtered data references without cloning
                        let data = data_arc.lock().unwrap();

                        // Create an iterator of references to valid rows instead of cloning them
                        let filtered_data_refs: Vec<&Vec<f64>> = data
                            .iter()
                            .filter(|row| {
                                row.iter().enumerate().all(|(i, val)| {
                                    i < filters_ref.len()
                                        && *val >= filters_ref[i].2
                                        && *val <= filters_ref[i].3
                                })
                            })
                            .collect();

                        // Grab the lock only for the UI part
                        if let Ok(mut settings) = settings_arc.lock() {
                            // Use an area with scrolling to ensure controls and plot fit
                            egui::ScrollArea::vertical()
                                .max_height(f32::INFINITY)
                                .show(ui, |ui| {
                                    Self::show_scatter_plot(ui, &filtered_data_refs, &mut settings);
                                });
                        } else {
                            ui.label("Settings currently unavailable");
                        }
                    });

                    // Only request repaints when necessary
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                },
            );
        }

        // Handle histogram windows
        let mut histogram_to_remove = Vec::new();

        // Handle existing histogram windows
        for (i, (is_open, settings)) in self.histograms.iter_mut().enumerate() {
            if !*is_open {
                histogram_to_remove.push(i);
                continue;
            }

            // Create a unique ID for each window
            let viewport_id = egui::ViewportId::from_hash_of(&format!("histogram_{}", i));
            let window_title = format!("Histogram {}", i + 1);

            // Clone the shared references for the window
            let settings_arc = settings.clone();
            let data_arc = self.data.clone();
            let filters_ref = &self.filters; // Use reference instead of cloning
            let _data_version = self._data_version;

            // Store a reference to is_open to track window state
            let is_open_ref = is_open;

            // Show the window using immediate viewport
            ctx.show_viewport_immediate(
                viewport_id,
                egui::ViewportBuilder::default()
                    .with_title(window_title)
                    .with_inner_size([900.0, 700.0]),
                move |ctx, _| {
                    // Check if the window's close button was clicked
                    ctx.input(|i| {
                        if i.viewport().close_requested() {
                            *is_open_ref = false;
                        }
                    });

                    // This closure runs every frame for the viewport
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_space(10.0);
                        // Get filtered data references without cloning
                        let data = data_arc.lock().unwrap();

                        // Create an iterator of references to valid rows instead of cloning them
                        let filtered_data_refs: Vec<&Vec<f64>> = data
                            .iter()
                            .filter(|row| {
                                row.iter().enumerate().all(|(i, val)| {
                                    i < filters_ref.len()
                                        && *val >= filters_ref[i].2
                                        && *val <= filters_ref[i].3
                                })
                            })
                            .collect();

                        // Grab the lock only for the UI part
                        if let Ok(mut settings) = settings_arc.lock() {
                            // Use an area with scrolling to ensure controls and plot fit
                            egui::ScrollArea::vertical()
                                .max_height(f32::INFINITY)
                                .show(ui, |ui| {
                                    Self::show_histogram(
                                        ui,
                                        &filtered_data_refs,
                                        &filters_ref,
                                        &mut settings,
                                        _data_version,
                                    );
                                });
                        } else {
                            ui.label("Settings currently unavailable");
                        }
                    });

                    // Only request repaints when necessary
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                },
            );
        }

        // Remove any closed plots
        for &idx in scatter_to_remove.iter().rev() {
            self.scatter_plots.swap_remove(idx);
        }

        // Remove any closed histograms
        for &idx in histogram_to_remove.iter().rev() {
            self.histograms.swap_remove(idx);
        }
    }
}

impl MainApp {
    fn show_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Interactive Data Explorer");
            ui.add_space(10.0);

            let row_count = self.data.lock().unwrap().len();
            let filtered_count = self.get_filtered_data_count();
            ui.vertical(|ui| {
                ui.strong("Data Summary");
                ui.label(format!("{} total rows", row_count));
                ui.label(format!("{} filtered rows", filtered_count));
            });
            ui.add_space(10.0);

            if ui.button("Create New Scatter Plot").clicked() {
                self.open_new_scatter_plot();
            }

            if ui.button("Create Histogram").clicked() {
                self.open_new_histogram();
            }

            ui.add_space(10.0);

            ui.heading("Filters");
            let column_count = {
                let data = self.data.lock().unwrap();
                data.first().map_or(0, |row| row.len())
            };

            if column_count > 0 {
                ui.separator();
                let stats = self.compute_statistics();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, filter) in self.filters.iter_mut().enumerate() {
                        ui.strong(format!("Column {}", i));
                        let range = filter.0..=filter.1;
                        ui.add(
                            egui::widgets::Slider::new(&mut filter.2, range.clone()).text("min"),
                        );
                        ui.add(egui::widgets::Slider::new(&mut filter.3, range).text("max"));
                        ui.label(format!("Average: {:.2}", stats[i].0));
                        ui.label(format!("Std Dev: {:.2}", stats[i].1));
                        ui.separator();
                    }
                });
            } else {
                ui.horizontal_centered(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label("Loading data...");
                });
            }
        });
    }

    fn open_new_scatter_plot(&mut self) {
        // Create default settings for the new plot
        let settings = Arc::new(Mutex::new(ScatterSettings::default()));

        // Add to our list of plots
        self.scatter_plots.push((true, settings));
    }

    fn open_new_histogram(&mut self) {
        // Create default settings for the new histogram
        let settings = Arc::new(Mutex::new(HistogramSettings::default()));

        // Add to our list of histograms
        self.histograms.push((true, settings));
    }

    fn show_scatter_plot(ui: &mut egui::Ui, data: &[&Vec<f64>], settings: &mut ScatterSettings) {
        if data.is_empty() {
            ui.label("No data to display");
            return;
        }

        // Display settings at the top with improved layout
        let column_count = data.first().map_or(0, |row| row.len());
        let col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();

        ui.add_space(10.0);

        // All controls in a single horizontal layout
        ui.horizontal(|ui| {
            // X column with label before the dropdown
            ui.label("X:");
            ui.add_space(2.0);

            let mut x_col = Some(settings.x_col);
            egui::ComboBox::new("x_column_combo", "")
                .selected_text(x_col.map_or("None".into(), |col| col.to_string()))
                .width(80.0) // Set a fixed width for the combo box
                .show_ui(ui, |ui| {
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut x_col, Some(i), item);
                    }
                });
            if let Some(col) = x_col {
                settings.x_col = col;
            }

            ui.add_space(8.0);

            // Y column with label before the dropdown
            ui.label("Y:");
            ui.add_space(2.0);

            let mut y_col = Some(settings.y_col);
            egui::ComboBox::new("y_column_combo", "")
                .selected_text(y_col.map_or("None".into(), |col| col.to_string()))
                .width(80.0) // Set a fixed width for the combo box
                .show_ui(ui, |ui| {
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut y_col, Some(i), item);
                    }
                });
            if let Some(col) = y_col {
                settings.y_col = col;
            }

            ui.add_space(8.0);

            // Color column with label before the dropdown
            ui.label("Color:");
            ui.add_space(2.0);

            let mut color_col = settings.color_col;
            egui::ComboBox::new("color_column_combo", "")
                .selected_text(color_col.map_or("None".into(), |col| col.to_string()))
                .width(80.0) // Set a fixed width for the combo box
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut color_col, None, "None");
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut color_col, Some(i), item);
                    }
                });
            settings.color_col = color_col;

            ui.add_space(8.0);

            // Size column with label before the dropdown
            ui.label("Size:");
            ui.add_space(2.0);

            let mut size_col = settings.size_col;
            egui::ComboBox::new("size_column_combo", "")
                .selected_text(size_col.map_or("None".into(), |col| col.to_string()))
                .width(80.0) // Set a fixed width for the combo box
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut size_col, None, "None");
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut size_col, Some(i), item);
                    }
                });
            settings.size_col = size_col;
        });

        ui.add_space(10.0);

        ui.separator();

        // Update color and size arrays
        // The generate_visual_array function already uses filtered data for normalization
        // when filters change, the data parameter contains only filtered rows
        // so color and size mappings are automatically recalculated
        let color_array = if let Some(col) = settings.color_col {
            Self::generate_visual_array(data, col, |norm_value| {
                let r = (255.0 * norm_value).round() as u8;
                let g = (norm_value * 128.0).round() as u8;
                let b = (0.0 + 255.0 * (1.0 - norm_value)).round() as u8;
                Color32::from_rgb(r, g, b)
            })
        } else {
            Vec::new()
        };

        let size_array = if let Some(col) = settings.size_col {
            Self::generate_visual_array(data, col, |norm_value| 1.0 + 5.0 * norm_value)
        } else {
            Vec::new()
        };

        // Collect plot data
        let plot_data = Self::collect_plot_data(data, settings);

        // Get column names for the plot title
        let x_name = data
            .first()
            .and_then(|row| {
                if row.len() > settings.x_col {
                    Some(format!("Column {}", settings.x_col))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "X".to_string());

        let y_name = data
            .first()
            .and_then(|row| {
                if row.len() > settings.y_col {
                    Some(format!("Column {}", settings.y_col))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Y".to_string());

        // Display the plot with a descriptive title
        let plot_title = format!("{} vs {}", x_name, y_name);
        let plot = Plot::new(plot_title)
            .allow_boxed_zoom(true)
            .allow_drag(true)
            .show_grid(true)
            .show_axes(true)
            .coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default())
            .min_size(egui::vec2(500.0, 400.0)) // Set minimum plot size
            .label_formatter(move |name, value| {
                if !name.is_empty() {
                    format!("{}: {:.2}, {}: {:.2}", x_name, value.x, y_name, value.y)
                } else {
                    format!("{}: {:.2}, {}: {:.2}", x_name, value.x, y_name, value.y)
                }
            });

        plot.show(ui, |plot_ui| {
            // Group points by similar properties for more efficient rendering
            let mut size_color_groups: Vec<(Color32, f32, Vec<[f64; 2]>)> = Vec::new();

            for (i, (pos, _, size_val)) in plot_data.iter().enumerate() {
                let color = if !color_array.is_empty() {
                    color_array[i]
                } else {
                    Color32::GRAY
                };
                let size = size_val.map_or(2.0, |_| size_array.get(i).cloned().unwrap_or(2.0));

                // Find existing group or create new one
                let found = size_color_groups
                    .iter_mut()
                    .find(|(c, s, _)| *c == color && (*s - size as f32).abs() < 0.001);
                if let Some((_, _, points)) = found {
                    points.push(*pos);
                } else {
                    size_color_groups.push((color, size as f32, vec![*pos]));
                }
            }

            // Render each group with a single draw call
            for (color, size, positions) in size_color_groups {
                plot_ui.points(Points::new(positions).radius(size).color(color));
            }
        });
    }

    fn get_filtered_data_count(&self) -> usize {
        let data = self.data.lock().unwrap();
        // More efficient counting logic - avoid Option creation
        data.iter()
            .filter(|row| {
                row.iter().enumerate().all(|(i, val)| {
                    i < self.filters.len() && *val >= self.filters[i].2 && *val <= self.filters[i].3
                })
            })
            .count()
    }

    fn compute_statistics(&self) -> Vec<(f64, f64)> {
        let data = self.data.lock().unwrap();
        if data.is_empty() || data[0].is_empty() {
            return Vec::new();
        }

        let num_cols = data[0].len();
        let mut means = vec![0.0; num_cols];
        let mut m2s = vec![0.0; num_cols]; // For computing running variance
        let mut counts = vec![0; num_cols];

        // Single pass over data computing running mean and variance
        for row in data.iter() {
            // Check if row passes all filters
            if !row.iter().enumerate().all(|(i, val)| {
                i < self.filters.len() && *val >= self.filters[i].2 && *val <= self.filters[i].3
            }) {
                continue;
            }

            for (col, &value) in row.iter().enumerate() {
                counts[col] += 1;
                let delta = value - means[col];
                means[col] += delta / counts[col] as f64;
                let delta2 = value - means[col];
                m2s[col] += delta * delta2;
            }
        }

        // Calculate final statistics
        means
            .iter()
            .zip(m2s.iter())
            .zip(counts.iter())
            .map(|((mean, m2), &count)| {
                let std = if count > 1 {
                    (m2 / (count - 1) as f64).sqrt()
                } else {
                    0.0
                };
                (*mean, std)
            })
            .collect()
    }

    fn generate_visual_array<F, Output>(data: &[&Vec<f64>], column: usize, mapper: F) -> Vec<Output>
    where
        F: Fn(f64) -> Output,
    {
        // Find min and max in one pass to avoid creating intermediaries
        // This only considers currently filtered data
        let (min_value, max_value, has_data) = data.iter().filter_map(|row| row.get(column)).fold(
            (f64::INFINITY, f64::NEG_INFINITY, false),
            |(min, max, _), &val| (min.min(val), max.max(val), true),
        );

        if !has_data {
            return Vec::new();
        }

        let range = max_value - min_value;

        if range == 0.0 {
            return data
                .iter()
                .filter_map(|row| row.get(column))
                .map(|_| mapper(1.0))
                .collect();
        }

        data.iter()
            .filter_map(|row| {
                row.get(column).map(|&val| {
                    let norm_value = (val - min_value) / range;
                    mapper(norm_value)
                })
            })
            .collect()
    }

    fn collect_plot_data(
        data: &[&Vec<f64>],
        settings: &ScatterSettings,
    ) -> Vec<([f64; 2], Option<f64>, Option<f64>)> {
        data.iter()
            .filter_map(|row| {
                if row.len() > settings.x_col && row.len() > settings.y_col {
                    let color = settings.color_col.and_then(|c| row.get(c).copied());
                    let size = settings.size_col.and_then(|s| row.get(s).copied());
                    Some(([row[settings.x_col], row[settings.y_col]], color, size))
                } else {
                    None
                }
            })
            .collect()
    }

    fn show_histogram(
        ui: &mut egui::Ui,
        data: &[&Vec<f64>],
        filters: &Vec<(f64, f64, f64, f64)>,
        settings: &mut HistogramSettings,
        _data_version: usize,
    ) {
        if data.is_empty() {
            ui.label("No data to display");
            return;
        }

        // Show column selector and bin count at the top
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            // Column dropdown
            ui.label("Column:");
            ui.add_space(2.0);

            let column_count = data.first().map_or(0, |row| row.len());
            let col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();

            let mut column = Some(settings.column);
            egui::ComboBox::new("histogram_column_combo", "")
                .selected_text(column.map_or("None".into(), |col| col.to_string()))
                .width(80.0)
                .show_ui(ui, |ui| {
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut column, Some(i), item);
                    }
                });
            if let Some(col) = column {
                settings.column = col;
            }

            ui.add_space(20.0);

            // Number of bins slider
            ui.label("Bins:");
            ui.add_space(2.0);
            ui.add(egui::Slider::new(&mut settings.bins, 5..=100).text(""));
        });

        ui.add_space(10.0);
        ui.separator();

        // Extract data for selected column - use references where possible
        let column_data: Vec<f64> = data
            .iter()
            .filter_map(|row| row.get(settings.column).copied())
            .collect();

        // Check if we need to recalculate statistics
        let needs_recalculation =
            settings.last_data_version != _data_version || settings.cached_stats.is_none();

        if column_data.is_empty() {
            ui.label("No data available for selected column");
            return;
        }

        // Calculate histogram bins
        let min_value = filters[settings.column].2;
        let max_value = filters[settings.column].3;
        let range = max_value - min_value;
        let bin_width = range / settings.bins as f64;

        // Count values in each bin
        let mut bin_counts = vec![0; settings.bins];
        for &value in &column_data {
            let bin_index = ((value - min_value) / bin_width).floor() as usize;
            let clamped_index = bin_index.min(settings.bins - 1);
            bin_counts[clamped_index] += 1;
        }

        // Calculate percentiles (25th, 50th, 75th) - only if data changed
        let (p25, p50, p75) = if needs_recalculation {
            let mut sorted_data = column_data.clone();
            // This is an expensive operation - only do it when needed
            sorted_data
                .sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let p25_index = (sorted_data.len() as f64 * 0.25) as usize;
            let p50_index = (sorted_data.len() as f64 * 0.5) as usize;
            let p75_index = (sorted_data.len() as f64 * 0.75) as usize;

            (
                sorted_data.get(p25_index).copied(),
                sorted_data.get(p50_index).copied(),
                sorted_data.get(p75_index).copied(),
            )
        } else {
            // Return cached values from previous calculation
            (None, None, None) // This needs proper implementation when you have caching for percentiles
        };

        // Create bar chart data
        let bars: Vec<Bar> = bin_counts
            .iter()
            .enumerate()
            .map(|(i, &count)| {
                let bin_start = min_value + i as f64 * bin_width;
                let bin_center = bin_start + bin_width / 2.0;

                Bar::new(bin_center, count as f64)
                    .width(bin_width * 0.95)
                    .fill(colors::DEFAULT_BAR_COLOR)
                    .name(format!("{:.2} - {:.2}", bin_start, bin_start + bin_width))
            })
            .collect();

        let chart = BarChart::new(bars);

        // Create and show the plot
        let column_name = format!("Column {}", settings.column);

        Plot::new(format!("Histogram of {}", column_name))
            .legend(Legend::default())
            .show_grid(true)
            .show_axes(true)
            .allow_boxed_zoom(true)
            .allow_drag(true)
            .x_axis_label(column_name)
            .y_axis_label("Count")
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(chart);

                // Show percentile lines
                if let Some(x) = p25 {
                    plot_ui.vline(
                        VLine::new(x)
                            .color(colors::PERCENTILE_25_COLOR)
                            .name(format!("25th percentile: {:.4}", x)),
                    );
                }

                if let Some(x) = p50 {
                    plot_ui.vline(
                        VLine::new(x)
                            .color(colors::PERCENTILE_50_COLOR)
                            .name(format!("50th percentile: {:.4}", x)),
                    );
                }

                if let Some(x) = p75 {
                    plot_ui.vline(
                        VLine::new(x)
                            .color(colors::PERCENTILE_75_COLOR)
                            .name(format!("75th percentile: {:.4}", x)),
                    );
                }
            });

        // Show statistics
        ui.separator();
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.strong("Statistics:");
                ui.label(format!("Count: {}", column_data.len()));
                ui.label(format!("Min: {:.4}", min_value));
                ui.label(format!("Max: {:.4}", max_value));
            });

            ui.add_space(40.0);

            ui.vertical(|ui| {
                ui.strong("Percentiles:");
                ui.label(format!("25th: {:.4}", p25.unwrap_or(f64::NAN)));
                ui.label(format!("50th: {:.4}", p50.unwrap_or(f64::NAN)));
                ui.label(format!("75th: {:.4}", p75.unwrap_or(f64::NAN)));
            });

            ui.add_space(40.0);

            // Use cached statistics if available and data hasn't changed
            let (mean, std_dev) =
                if settings.last_data_version != _data_version || settings.cached_stats.is_none() {
                    // Calculate statistics and cache them
                    let mean = column_data.iter().sum::<f64>() / column_data.len() as f64;
                    let variance = column_data.iter().map(|&x| (x - mean).powi(2)).sum::<f64>()
                        / column_data.len() as f64;
                    let std_dev = variance.sqrt();

                    settings.cached_stats = Some((mean, variance, std_dev));
                    settings.last_data_version = _data_version;

                    (mean, std_dev)
                } else {
                    // Use cached values
                    let (mean, _variance, std_dev) = settings.cached_stats.unwrap();
                    (mean, std_dev)
                };

            ui.vertical(|ui| {
                ui.strong("Distribution:");
                ui.label(format!("Mean: {:.4}", mean));
                ui.label(format!("Std Dev: {:.4}", std_dev));
                ui.label(format!("Range: {:.4}", range));
            });
        });
    }
}
