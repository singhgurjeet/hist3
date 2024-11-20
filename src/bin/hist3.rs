#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{Bar, BarChart, Legend, Plot};
use hist3::data;
use hist3::data::InputSource;
use std::path::Path;

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Number of bins
    #[arg(long, short, default_value_t = 20)]
    bins: usize,

    /// Title
    #[arg(long, short, default_value = "Histogram")]
    title: String,
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone();

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

    let (labels_and_counts, p_25, p_50, p_75, total, range) =
        data::compute_histogram(args.bins, input);

    let plot = PlotApp::new(labels_and_counts, p_25, p_50, p_75, total, range, args.bins);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(title.as_str(), options, Box::new(|_| Ok(Box::new(plot))))
}

struct PlotApp {
    data: Vec<(String, usize)>,
    p_25: Option<(f64, f64)>,
    p_50: Option<(f64, f64)>,
    p_75: Option<(f64, f64)>,
    total: f64,
    range: f64,
    num_bins: usize,
    grid: bool,
    axes: bool,
}

impl PlotApp {
    fn new(
        data: Vec<(String, usize)>,
        p_25: Option<(f64, f64)>,
        p_50: Option<(f64, f64)>,
        p_75: Option<(f64, f64)>,
        total: f64,
        range: f64,
        num_bins: usize,
    ) -> Self {
        PlotApp {
            data,
            p_25,
            p_50,
            p_75,
            total,
            range,
            num_bins,
            grid: true,
            axes: true,
        }
    }

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
        let width = if self.p_25.is_some() {
            self.range / self.num_bins as f64
        } else {
            1.0
        };
        let chart = BarChart::new(
            self.data
                .iter()
                .enumerate()
                .map(|(i, (label, count))| {
                    if self.p_25.is_some() {
                        Bar::new(label.parse::<f64>().unwrap(), *count as f64)
                            .width(width)
                            .name(label)
                    } else {
                        Bar::new(i as f64, *count as f64).width(1.0).name(label)
                    }
                })
                .collect(),
        );

        egui::CentralPanel::default().show(ctx, |ui| {
            Plot::new("")
                .allow_boxed_zoom(true)
                .allow_drag(false)
                .legend(Legend::default())
                .show_grid(self.grid)
                .show_axes(self.axes)
                .show(ui, |plot_ui| plot_ui.bar_chart(chart));
        });
    }
}
