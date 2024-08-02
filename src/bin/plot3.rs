#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{CoordinatesFormatter, Corner, Legend, Line, Plot, PlotPoints};
use hist3::data::InputSource;
use hist3::NUMRE;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{io, thread};

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Title
    #[arg(long, short, default_value = "Plot")]
    title: String,

    /// Series Names
    #[arg(short, long)]
    series: Vec<String>,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();

    let plot = PlotApp::default()
        .set_series_names(args.series.clone())
        .set_grid(true)
        .set_axes(true);
    let data_ref = plot.data.clone();
    let title = args.title.clone();

    thread::spawn(move || {
        let input = if !atty::is(Stream::Stdin) {
            InputSource::Stdin
        } else {
            let file_name = args
                .input
                .expect("Input must either be piped in or provide a file")
                .to_owned();
            if !Path::new(&file_name).exists() {
                panic!("File does not exist");
            }
            InputSource::FileName(file_name)
        };

        match input {
            InputSource::Stdin => {
                let reader = std::io::stdin();
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(&data_ref, line);
                    }
                }
            }
            InputSource::FileName(file_name) => {
                let file = File::open(file_name).unwrap();
                let reader = io::BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(&data_ref, line);
                    }
                }
            }
        };
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(title.as_str(), options, Box::new(|_| Box::new(plot)))
}

fn process_line(data_ref: &Arc<Mutex<Vec<Vec<f64>>>>, line: String) {
    let floats = NUMRE
        .captures_iter(&line)
        .map(|cap| cap[0].parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    if floats.len() > 0 {
        data_ref.lock().unwrap().push(floats);
    }
}

struct PlotApp {
    data: Arc<Mutex<Vec<Vec<f64>>>>,
    grid: bool,
    axes: bool,
    cums: Vec<bool>,
    normalize: Vec<bool>,
    box_width: Vec<usize>,
    series_names: Vec<String>,
}

impl Default for PlotApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            grid: false,
            axes: false,
            cums: Vec::new(),
            normalize: Vec::new(),
            box_width: Vec::new(),
            series_names: Vec::new(),
        }
    }
}

impl PlotApp {
    fn set_grid(mut self, grid: bool) -> Self {
        self.grid = grid;
        self
    }

    fn set_axes(mut self, axes: bool) -> Self {
        self.axes = axes;
        self
    }

    fn set_series_names(mut self, series_names: Vec<String>) -> Self {
        self.series_names = series_names;
        self
    }
}

impl eframe::App for PlotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.data.lock().unwrap().len() == 0 {
            return;
        }
        let num_series = self.data.lock().unwrap()[0].len();
        while self.cums.len() < num_series {
            self.cums.push(false);
            self.normalize.push(false);
            self.box_width.push(1);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                for i in 0..num_series {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.cums[i], "Cumulative");
                                ui.add(
                                    egui::DragValue::new(&mut self.box_width[i])
                                        .clamp_range(1..=50000),
                                );
                                ui.label("Averaging");
                                ui.checkbox(&mut self.normalize[i], "Normalize");
                                ui.heading(format!(
                                    "{}",
                                    self.series_names.get(i).unwrap_or(&format!("{}", i))
                                ));
                            })
                        })
                    });
                }
            });

            let mut plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .legend(Legend::default())
                .show_grid(self.grid)
                .show_axes(self.axes);
            plot = plot.coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());
            plot.show(ui, |plot_ui| {
                for i in 0..num_series {
                    plot_ui.line(
                        Line::new(PlotPoints::from_ys_f64(&make_series(
                            &self.data.lock().unwrap(),
                            i,
                            self.box_width[i],
                            self.cums[i],
                            self.normalize[i],
                        )))
                        .name(format!(
                            "{}",
                            self.series_names.get(i).unwrap_or(&format!("{}", i))
                        )),
                    )
                }
            });
        });
    }
}

fn make_series(
    data: &Vec<Vec<f64>>,
    series_idx: usize,
    width: usize,
    cumulative: bool,
    normalize: bool,
) -> Vec<f64> {
    let min = data
        .iter()
        .map(|v| v[series_idx])
        .min_by(|a, b| a.total_cmp(b))
        .unwrap();
    let max = data
        .iter()
        .map(|v| v[series_idx])
        .max_by(|a, b| a.total_cmp(b))
        .unwrap();
    let range = max - min;
    data.iter()
        .enumerate()
        .map(|(i, _)| {
            if width > 1 {
                let start = if i >= width / 2 { i - width / 2 } else { 0 };
                let end = std::cmp::min(data.len(), i + width / 2 + 1);
                let sum: f64 = data[start..end]
                    .iter()
                    .map(|v| {
                        if normalize {
                            ((v[series_idx] - min) / range) * 2.0 - 1.0
                        } else {
                            v[series_idx]
                        }
                    })
                    .sum();
                let count = end - start;
                sum / count as f64
            } else {
                if normalize {
                    ((data[i][series_idx] - min) / range) * 2.0 - 1.0
                } else {
                    data[i][series_idx]
                }
            }
        })
        .scan(0.0, |cum, v| {
            if cumulative {
                *cum += v;
                Some(*cum)
            } else {
                Some(v)
            }
        })
        .collect()
}
