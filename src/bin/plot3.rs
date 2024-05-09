#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{Legend, Line, Plot, PlotPoints};
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
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();

    let plot = PlotApp::default();
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

        match input {
            InputSource::Stdin => {
                let reader = std::io::stdin();
                for line in reader.lines() {
                    if let Ok(line) = line {
                        data_ref
                            .lock()
                            .unwrap()
                            .push(line.parse::<f64>().unwrap_or(0.0));
                    }
                }
            }
            InputSource::FileName(file_name) => {
                let file = File::open(file_name).unwrap();
                let reader = io::BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        data_ref
                            .lock()
                            .unwrap()
                            .push(line.parse::<f64>().unwrap_or(0.0));
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

struct PlotApp {
    data: Arc<Mutex<Vec<f64>>>,
}

impl Default for PlotApp {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl eframe::App for PlotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .legend(Legend::default())
                .show_grid(false)
                .show_axes(false)
                .show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(PlotPoints::from_ys_f64(&self.data.lock().unwrap())).name("1"),
                    )
                });
        });
    }
}
