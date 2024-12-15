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
    #[arg(long, short, default_value = "Histogram")]
    title: String,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone();

    let plot = XApp::default();
    let data_ref = plot.data.clone();

    thread::spawn(move || {
        let input = get_input_source(&args);
        process_input(input, &data_ref);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0]) // Wider default window
            .with_min_inner_size([400.0, 300.0]), // Set minimum size
        ..Default::default()
    };
    eframe::run_native(title.as_str(), options, Box::new(|_| Ok(Box::new(plot))))
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

struct XApp {
    data: Arc<Mutex<Vec<Vec<f64>>>>,
    x_col: usize,
    y_col: usize,
    color_col: Option<usize>,
    size_col: Option<usize>,
}

impl Default for XApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            x_col: 0,
            y_col: 1,
            color_col: None,
            size_col: None,
        }
    }
}

impl eframe::App for XApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            let column_count = {
                let data = self.data.lock().unwrap();
                data.first().map_or(0, |row| row.len())
            };

            let col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();

            egui::ComboBox::from_label("X Column")
                .selected_text(self.x_col.to_string())
                .show_ui(ui, |ui| {
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.x_col, i, item);
                    }
                });

            egui::ComboBox::from_label("Y Column")
                .selected_text(self.y_col.to_string())
                .show_ui(ui, |ui| {
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.y_col, i, item);
                    }
                });

            egui::ComboBox::from_label("Color Column")
                .selected_text(self.color_col.map_or("None".into(), |col| col.to_string()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.color_col, None, "None");
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.color_col, Some(i), item);
                    }
                });

            egui::ComboBox::from_label("Size Column")
                .selected_text(self.size_col.map_or("None".into(), |col| col.to_string()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.size_col, None, "None");
                    for (i, item) in col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.size_col, Some(i), item);
                    }
                });
        });

        let simple_gradient = |value: f64| -> Color32 {
            let norm = value.max(0.0).min(1.0);
            let r = (0.0 + 255.0 * (1.0 - norm)).round() as u8;
            let g = (norm * 128.0).round() as u8;
            let b = (255.0 * norm).round() as u8;
            Color32::from_rgb(r, g, b)
        };

        let color_array = if let Some(color_col) = self.color_col {
            let data = self.data.lock().unwrap();
            let values = data
                .iter()
                .filter_map(|row| row.get(color_col))
                .cloned()
                .collect::<Vec<_>>();

            let min_value = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_value = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = max_value - min_value;

            values
                .iter()
                .map(|&val| {
                    let norm_value = (val - min_value) / range; // Normalize to 0.0 - 1.0
                    simple_gradient(norm_value)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let size_array = if let Some(size_col) = self.size_col {
            let data = self.data.lock().unwrap();
            let values = data
                .iter()
                .filter_map(|row| row.get(size_col))
                .cloned()
                .collect::<Vec<_>>();

            let min_value = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_value = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = max_value - min_value;

            values
                .iter()
                .map(|&val| {
                    let norm_value = (val - min_value) / range; // Normalize to 0.0 - 1.0
                    1.0 + 9.0 * norm_value // Scale to 1.0 - 10.0
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let plot_data = {
            let data = self.data.lock().unwrap();
            data.iter()
                .filter_map(|row| {
                    if row.len() > self.x_col && row.len() > self.y_col {
                        let color = self.color_col.and_then(|c| row.get(c)).cloned();
                        let size = self.size_col.and_then(|s| row.get(s)).cloned();
                        Some(([row[self.x_col], row[self.y_col]], color, size))
                    } else {
                        None
                    }
                })
                .collect::<Vec<([f64; 2], Option<f64>, Option<f64>)>>()
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .show_grid(true)
                .show_axes(true);

            plot = plot.coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());
            plot.show(ui, |plot_ui| {
                for (i, (pos, _, size_val)) in plot_data.iter().enumerate() {
                    let color = if !color_array.is_empty() {
                        color_array[i]
                    } else {
                        Color32::WHITE
                    };
                    let size = size_val.map_or(2.0, |_| size_array.get(i).cloned().unwrap_or(2.0));

                    let points = Points::new(vec![*pos]).radius(size as f32).color(color);
                    plot_ui.points(points);
                }
            });
        });
    }
}
