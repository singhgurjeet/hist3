#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
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

    let plot = XApp::default().with_settings(true);
    let data_ref = plot.data.clone();

    thread::spawn(move || {
        let input = get_input_source(&args);
        process_input(input, &data_ref);
    });

    let options = eframe::NativeOptions {
        ..Default::default() // Correcting settings by just using default options
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
    grid: bool,
    x_col: usize,
    y_col: usize,
}

impl Default for XApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            grid: false,
            x_col: 0,
            y_col: 1,
        }
    }
}

impl XApp {
    fn with_settings(mut self, grid: bool) -> Self {
        self.grid = grid;
        self
    }
}

impl eframe::App for XApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.label("Select columns for X and Y axes:");

            let column_count = {
                let data = self.data.lock().unwrap();
                data.first().map_or(0, |row| row.len())
            };

            let x_col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();
            let y_col_items = (0..column_count).map(|i| i.to_string()).collect::<Vec<_>>();

            egui::ComboBox::from_label("X Column")
                .selected_text(self.x_col.to_string())
                .show_ui(ui, |ui| {
                    for (i, item) in x_col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.x_col, i, item);
                    }
                });

            egui::ComboBox::from_label("Y Column")
                .selected_text(self.y_col.to_string())
                .show_ui(ui, |ui| {
                    for (i, item) in y_col_items.iter().enumerate() {
                        ui.selectable_value(&mut self.y_col, i, item);
                    }
                });
        });

        let plot_data = {
            let data = self.data.lock().unwrap();
            data.iter()
                .filter_map(|row| {
                    if row.len() > self.x_col && row.len() > self.y_col {
                        Some([row[self.x_col], row[self.y_col]])
                    } else {
                        None
                    }
                })
                .collect::<Vec<[f64; 2]>>()
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .show_grid(self.grid)
                .show_axes(true);

            plot = plot.coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());
            plot.show(ui, |plot_ui| {
                plot_ui.points(
                    Points::new(plot_data.clone())
                        .radius(2.0)
                        .color(egui::Color32::from_rgb(75, 75, 75)),
                );
            });
        });
    }
}
