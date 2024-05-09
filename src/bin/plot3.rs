#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{CoordinatesFormatter, Corner, Legend, Line, Plot, PlotPoints};
use hist3::data::InputSource;
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

    /// Show grid?
    #[arg(long, short)]
    grid: bool,

    /// Show axes?
    #[arg(long, short)]
    axes: bool,

    /// Cumulative?
    #[arg(long, short)]
    cumulative: bool,

    /// Ema?
    #[arg(long, short)]
    ema_alpha: Option<f64>,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    if args.cumulative && args.ema_alpha.is_some() {
        panic!("cumulative and ema together are not supported, use one or the other");
    }

    let plot = PlotApp::default().set_grid(args.grid).set_axes(args.axes);
    let data_ref = plot.data.clone();

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

        let mut cumsum = 0.0;

        match input {
            InputSource::Stdin => {
                let reader = std::io::stdin();
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(
                            &data_ref,
                            line,
                            &mut cumsum,
                            args.cumulative,
                            args.ema_alpha,
                        );
                    }
                }
            }
            InputSource::FileName(file_name) => {
                let file = File::open(file_name).unwrap();
                let reader = io::BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(
                            &data_ref,
                            line,
                            &mut cumsum,
                            args.cumulative,
                            args.ema_alpha,
                        );
                    }
                }
            }
        };
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Plot", options, Box::new(|_| Box::new(plot)))
}

fn process_line(
    data_ref: &Arc<Mutex<Vec<f64>>>,
    line: String,
    cumsum: &mut f64,
    cumulative: bool,
    ema_alpha: Option<f64>,
) {
    if let Ok(val) = line.parse::<f64>() {
        let val = if cumulative {
            *cumsum += val;
            *cumsum
        } else {
            val
        };
        let val = if let Some(alpha) = ema_alpha {
            *cumsum = alpha * val + (1.0 - alpha) * *cumsum;
            *cumsum
        } else {
            val
        };
        data_ref.lock().unwrap().push(val);
    }
}

struct PlotApp {
    data: Arc<Mutex<Vec<f64>>>,
    grid: bool,
    axes: bool,
}

impl Default for PlotApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
            grid: false,
            axes: false,
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
}

impl eframe::App for PlotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut plot = Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .legend(Legend::default())
                .show_grid(self.grid)
                .show_axes(self.axes);
            plot = plot.coordinates_formatter(Corner::LeftBottom, CoordinatesFormatter::default());
            plot.show(ui, |plot_ui| {
                plot_ui
                    .line(Line::new(PlotPoints::from_ys_f64(&self.data.lock().unwrap())).name("1"))
            });
        });
    }
}
