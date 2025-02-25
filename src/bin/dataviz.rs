#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui::Color32;
use egui_plot::{CoordinatesFormatter, Corner, Plot, Points};
use hist3::data::InputSource;
use hist3::NUMERIC_REGEX;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;

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

    for line in reader.lines() {
        if let Ok(line) = line {
            let is_valid_line = first_line_size.map_or(true, |size| {
                NUMERIC_REGEX.captures_iter(&line).count() == size
            });

            if is_valid_line {
                let floats = NUMERIC_REGEX
                    .captures_iter(&line)
                    .map(|cap| f64::from_str(&cap[0]).unwrap())
                    .collect::<Vec<_>>();

                if first_line_size.is_none() {
                    first_line_size = Some(floats.len());
                }

                data_ref.lock().unwrap().push(floats);
            }
        }
    }
}

struct MainApp {
    data: Arc<Mutex<Vec<Vec<f64>>>>,
    filters: Vec<(f64, f64, f64, f64)>,
    scatter_plots: Vec<(bool, Arc<Mutex<ScatterSettings>>)>, // (is_open, settings)
}

#[derive(Clone)]
struct ScatterSettings {
    x_col: usize,
    y_col: usize,
    color_col: Option<usize>,
    size_col: Option<usize>,
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

impl Default for MainApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            filters: Vec::new(),
            scatter_plots: Vec::new(),
        }
    }
}

impl eframe::App for MainApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize filters if needed
        if self.filters.len() != self.data.lock().unwrap().first().map_or(0, |row| row.len()) {
            let data = self.data.lock().unwrap();
            let columns = data.first().map_or(0, |row| row.len());

            self.filters.clear();
            for col in 0..columns {
                let min = data
                    .iter()
                    .filter_map(|row| row.get(col))
                    .fold(f64::INFINITY, |min, &val| min.min(val));
                let max = data
                    .iter()
                    .filter_map(|row| row.get(col))
                    .fold(f64::NEG_INFINITY, |max, &val| max.max(val));
                self.filters.push((min, max, min, max));
            }
        }

        // Show the side panel with filters
        self.show_main_panel(ctx);

        // Show central panel with button
        // self.show_central_panel(ctx);

        // We don't need to calculate filtered data here since each window computes it

        // Handle scatter plot windows
        let mut to_remove = Vec::new();

        // First handle existing windows
        for (i, (is_open, settings)) in self.scatter_plots.iter_mut().enumerate() {
            if !*is_open {
                to_remove.push(i);
                continue;
            }

            // Create a unique ID for each window
            let viewport_id = egui::ViewportId::from_hash_of(&format!("scatter_plot_{}", i));
            let window_title = format!("Scatter Plot {}", i + 1);

            // Clone the shared references for the window
            let settings_arc = settings.clone();
            let data_arc = self.data.clone();
            let filters = self.filters.clone();

            // Show the window using immediate viewport
            ctx.show_viewport_immediate(
                viewport_id,
                egui::ViewportBuilder::default()
                    .with_title(window_title)
                    .with_inner_size([900.0, 700.0]),
                move |ctx, _| {
                    // This closure runs every frame for the viewport
                    egui::CentralPanel::default().show(ctx, |ui| {
                        // Get the filtered data based on the shared filters
                        let filtered_data = {
                            let data = data_arc.lock().unwrap();
                            data.iter()
                                .filter_map(|row| {
                                    if row.iter().enumerate().all(|(i, val)| {
                                        i < filters.len()
                                            && *val >= filters[i].2
                                            && *val <= filters[i].3
                                    }) {
                                        Some(row.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                        };

                        // Grab the lock only for the UI part
                        if let Ok(mut settings) = settings_arc.lock() {
                            // Use an area with scrolling to ensure controls and plot fit
                            egui::ScrollArea::vertical()
                                .max_height(f32::INFINITY)
                                .show(ui, |ui| {
                                    Self::show_scatter_plot(ui, &filtered_data, &mut settings);
                                });
                        } else {
                            ui.label("Settings currently unavailable");
                        }
                    });

                    // Ensure continuous rendering
                    ctx.request_repaint();
                },
            );
        }

        // Remove any closed plots
        for &idx in to_remove.iter().rev() {
            self.scatter_plots.swap_remove(idx);
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

            ui.heading("Filters");
            let column_count = {
                let data = self.data.lock().unwrap();
                data.first().map_or(0, |row| row.len())
            };

            if column_count > 0 {
                ui.separator();
                let stats = self.compute_statistics();

                for (i, filter) in self.filters.iter_mut().enumerate() {
                    ui.strong(format!("Column {}", i));
                    let range = filter.0..=filter.1;
                    ui.add(egui::widgets::Slider::new(&mut filter.2, range.clone()).text("min"));
                    ui.add(egui::widgets::Slider::new(&mut filter.3, range).text("max"));
                    ui.label(format!("Average: {:.2}", stats[i].0));
                    ui.label(format!("Std Dev: {:.2}", stats[i].1));
                    ui.separator();
                }

                ui.add_space(20.0);

                if ui.button("Create New Scatter Plot").clicked() {
                    self.open_new_scatter_plot();
                }

                ui.add_space(10.0);

                if !self.scatter_plots.is_empty() {
                    ui.separator();
                    ui.heading("Active Plots");
                    ui.add_space(5.0);

                    for (i, (is_open, _)) in self.scatter_plots.iter().enumerate() {
                        if *is_open {
                            ui.horizontal(|ui| {
                                ui.label(format!("• Scatter Plot {}", i + 1));
                                ui.label("(in separate window)");
                            });
                        }
                    }

                    ui.add_space(5.0);
                    ui.label(
                        "Note: Each scatter plot window updates automatically with filter changes.",
                    );
                }
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

    fn show_scatter_plot(ui: &mut egui::Ui, data: &[Vec<f64>], settings: &mut ScatterSettings) {
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
            for (i, (pos, _, size_val)) in plot_data.iter().enumerate() {
                let color = if !color_array.is_empty() {
                    color_array[i]
                } else {
                    Color32::GRAY
                };
                let size = size_val.map_or(2.0, |_| size_array.get(i).cloned().unwrap_or(2.0));

                let points = Points::new(vec![*pos]).radius(size as f32).color(color);
                plot_ui.points(points);
            }
        });
    }

    fn get_filtered_data_count(&self) -> usize {
        let data = self.data.lock().unwrap();
        data.iter()
            .filter_map(|row| {
                if row.iter().enumerate().all(|(i, val)| {
                    i < self.filters.len() && *val >= self.filters[i].2 && *val <= self.filters[i].3
                }) {
                    Some(1)
                } else {
                    None
                }
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

    fn generate_visual_array<F, Output>(data: &[Vec<f64>], column: usize, mapper: F) -> Vec<Output>
    where
        F: Fn(f64) -> Output,
    {
        let values: Vec<f64> = data
            .iter()
            .filter_map(|row| row.get(column))
            .cloned()
            .collect();

        if values.is_empty() {
            return Vec::new();
        }

        let min_value = values.iter().fold(f64::INFINITY, |min, &val| min.min(val));
        let max_value = values
            .iter()
            .fold(f64::NEG_INFINITY, |max, &val| max.max(val));
        let range = max_value - min_value;

        if range == 0.0 {
            return values.iter().map(|_| mapper(1.0)).collect();
        }

        values
            .iter()
            .map(|&val| {
                let norm_value = (val - min_value) / range;
                mapper(norm_value)
            })
            .collect()
    }

    fn collect_plot_data(
        data: &[Vec<f64>],
        settings: &ScatterSettings,
    ) -> Vec<([f64; 2], Option<f64>, Option<f64>)> {
        data.iter()
            .filter_map(|row| {
                if row.len() > settings.x_col && row.len() > settings.y_col {
                    let color = settings.color_col.and_then(|c| row.get(c)).cloned();
                    let size = settings.size_col.and_then(|s| row.get(s)).cloned();
                    Some(([row[settings.x_col], row[settings.y_col]], color, size))
                } else {
                    None
                }
            })
            .collect()
    }
}
