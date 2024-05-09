#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{Legend, Plot, Points};
use hist3::data::InputSource;
use regex::Regex;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::str::FromStr;
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
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();

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

        let re = Regex::new(r"[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?").unwrap();
        match input {
            InputSource::Stdin => {
                let reader = std::io::stdin();
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(&data_ref, &re, &line);
                    }
                }
            }
            InputSource::FileName(file_name) => {
                let file = File::open(file_name).unwrap();
                let reader = io::BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        process_line(&data_ref, &re, &line);
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

fn process_line(data_ref: &Arc<Mutex<Vec<[f64; 2]>>>, re: &Regex, line: &String) {
    let floats = re
        .captures_iter(&line)
        .map(|cap| f64::from_str(&cap[0]).unwrap())
        .collect::<Vec<_>>();
    let coords = floats.iter().rev().take(2).collect::<Vec<_>>();
    if coords.len() == 2 {
        data_ref.lock().unwrap().push([*coords[1], *coords[0]]);
    }
}

struct PlotApp {
    data: Arc<Mutex<Vec<[f64; 2]>>>,
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
            Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .legend(Legend::default())
                .show_grid(self.grid)
                .show_axes(self.axes)
                .show(ui, |plot_ui| {
                    plot_ui.points(
                        Points::new(self.data.lock().unwrap().clone())
                            .radius(2.0)
                            .name("1"),
                    );
                });
        });
    }
}
