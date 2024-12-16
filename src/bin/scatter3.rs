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
    #[arg(long, short, default_value = "Scatter Plot")]
    title: String,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone();

    let plot = ScatterApp::default();
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

struct ScatterApp {
    data: Arc<Mutex<Vec<Vec<f64>>>>,
    x_col: usize,
    y_col: usize,
    color_col: Option<usize>,
    size_col: Option<usize>,
    color_cache: HashMap<usize, Vec<Color32>>,
    size_cache: HashMap<usize, Vec<f64>>,
}

impl Default for ScatterApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            x_col: 0,
            y_col: 1,
            color_col: None,
            size_col: None,
            color_cache: HashMap::new(),
            size_cache: HashMap::new(),
        }
    }
}

impl eframe::App for ScatterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_side_panel(ctx);

        if self.color_col.is_some() && !self.color_cache.contains_key(&self.color_col.unwrap()) {
            let color_array = self.generate_color_array();
            self.color_cache
                .insert(self.color_col.unwrap(), color_array);
        }

        if self.size_col.is_some() && !self.size_cache.contains_key(&self.size_col.unwrap()) {
            let size_array = self.generate_size_array();
            self.size_cache.insert(self.size_col.unwrap(), size_array);
        }

        let plot_data = self.collect_plot_data();

        let color_array = self
            .color_col
            .and_then(|col| self.color_cache.get(&col))
            .cloned()
            .unwrap_or_default();
        let size_array = self
            .size_col
            .and_then(|col| self.size_cache.get(&col))
            .cloned()
            .unwrap_or_default();

        self.show_central_panel(ctx, &plot_data, &color_array, &size_array);
    }
}

impl ScatterApp {
    fn show_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            let column_count = {
                let data = self.data.lock().unwrap();
                data.first().map_or(0, |row| row.len())
            };

            let col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();

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
                self.color_cache
                    .remove(&self.color_col.unwrap_or(usize::MAX));
            }

            if size_col != self.size_col {
                self.size_col = size_col;
                self.size_cache.remove(&self.size_col.unwrap_or(usize::MAX));
            }

            self.x_col = x_col.unwrap_or(0);
            self.y_col = y_col.unwrap_or(1);
        });
    }

    fn create_combo_box(
        &mut self,
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

    fn generate_color_array(&self) -> Vec<Color32> {
        self.generate_visual_array(self.color_col, |norm_value| {
            let r = (255.0 * norm_value).round() as u8;
            let g = (norm_value * 128.0).round() as u8;
            let b = (0.0 + 255.0 * (1.0 - norm_value)).round() as u8;
            Color32::from_rgb(r, g, b)
        })
    }

    fn generate_size_array(&self) -> Vec<f64> {
        self.generate_visual_array(self.size_col, |norm_value| 1.0 + 5.0 * norm_value)
    }

    fn generate_visual_array<F, Output>(&self, column: Option<usize>, mapper: F) -> Vec<Output>
    where
        F: Fn(f64) -> Output,
    {
        if let Some(col) = column {
            let data = self.data.lock().unwrap();
            let values = data
                .iter()
                .filter_map(|row| row.get(col))
                .cloned()
                .collect::<Vec<_>>();

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
        } else {
            Vec::new()
        }
    }

    fn collect_plot_data(&self) -> Vec<([f64; 2], Option<f64>, Option<f64>)> {
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
            .collect()
    }

    fn show_central_panel(
        &self,
        ctx: &egui::Context,
        plot_data: &Vec<([f64; 2], Option<f64>, Option<f64>)>,
        color_array: &Vec<Color32>,
        size_array: &Vec<f64>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .show_grid(true)
                .show_axes(true)
                .coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());

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
        });
    }
}
