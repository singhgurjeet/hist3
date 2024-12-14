#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{Bar, BarChart, Legend, Plot};
use hist3::data;
use hist3::data::InputSource;
use std::iter::FromIterator;
use std::ops::RangeInclusive;
use std::path::Path;

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Categorical input
    #[arg(long, short)]
    categorical: bool,

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
        data::compute_histogram(args.bins, input, args.categorical);

    let plot = PlotApp::new(labels_and_counts, p_25, p_50, p_75, total, range, args.bins);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0]) // Wider default window
            .with_min_inner_size([400.0, 300.0]), // Set minimum size
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
    selection: Option<RangeInclusive<usize>>,
    drag_start: Option<egui_plot::PlotPoint>,
    drag_end: Option<egui_plot::PlotPoint>,
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
            selection: None,
            drag_start: None,
            drag_end: None,
        }
    }

    fn is_bar_in_rect(
        &self,
        bar_idx: usize,
        start: &egui_plot::PlotPoint,
        end: &egui_plot::PlotPoint,
    ) -> bool {
        let bar_x = if self.p_25.is_some() {
            self.data[bar_idx].0.parse::<f64>().unwrap()
        } else {
            bar_idx as f64
        };
        let bar_y = self.data[bar_idx].1 as f64;

        let x_min = start.x.min(end.x);
        let x_max = start.x.max(end.x);
        let y_min = start.y.min(end.y);
        let y_max = start.y.max(end.y);

        bar_x >= x_min
            && bar_x <= x_max
            && ((bar_y >= y_min && bar_y <= y_max) || (y_min > 0.0 && bar_y >= y_max))
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
                    let mut bar = if self.p_25.is_some() {
                        Bar::new(label.parse::<f64>().unwrap(), *count as f64)
                            .width(width)
                            .name(label)
                    } else {
                        Bar::new(i as f64, *count as f64).width(1.0).name(label)
                    };

                    if let Some(range) = &self.selection {
                        if range.contains(&i) {
                            bar = bar.fill(egui::Color32::from_rgb(255, 165, 0));
                        }
                    }
                    bar
                })
                .collect(),
        );

        egui::CentralPanel::default().show(ctx, |ui| {
            Plot::new("")
                .allow_boxed_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .legend(Legend::default())
                .show_grid(self.grid)
                .show_axes(self.axes)
                .x_axis_label(" ")
                .label_formatter(|name, value| {
                    if !name.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{:.1}", value.x)
                    }
                })
                .show(ui, |plot_ui| {
                    if let Some(pointer) = plot_ui.pointer_coordinate() {
                        if plot_ui.ctx().input(|i| i.pointer.primary_pressed()) {
                            // Start drag
                            self.drag_start = Some(pointer);
                            self.drag_end = Some(pointer);
                        } else if plot_ui.ctx().input(|i| i.pointer.primary_down()) {
                            // Continue drag
                            self.drag_end = Some(pointer);
                        } else if plot_ui.ctx().input(|i| i.pointer.primary_released()) {
                            // End drag and select bars
                            if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                                let selected_bars: Vec<usize> = (0..self.data.len())
                                    .filter(|&i| self.is_bar_in_rect(i, &start, &end))
                                    .collect();

                                if !selected_bars.is_empty() {
                                    self.selection = Some(
                                        *selected_bars.first().unwrap()
                                            ..=*selected_bars.last().unwrap(),
                                    );
                                } else {
                                    self.selection = None;
                                }
                            }
                            self.drag_start = None;
                            self.drag_end = None;
                        }
                    }

                    plot_ui.bar_chart(chart.width(width * 0.92));

                    // Draw selection rectangle
                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                        plot_ui.polygon(
                            egui_plot::Polygon::new(egui_plot::PlotPoints::from_iter(vec![
                                [start.x, start.y],
                                [end.x, start.y],
                                [end.x, end.y],
                                [start.x, end.y],
                            ])), // .fill_color(egui::Color32::WHITE),
                        );
                    }

                    if let Some((_, x)) = self.p_25 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(egui::Color32::LIGHT_BLUE)
                                .name(format!("25 ptile: {:.4}", x)),
                        );
                    }
                    if let Some((_, x)) = self.p_50 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(egui::Color32::LIGHT_GREEN)
                                .name(format!("50 ptile: {:.4}", x)),
                        );
                    }
                    if let Some((_, x)) = self.p_75 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(egui::Color32::LIGHT_RED)
                                .name(format!("75 ptile: {:.4}", x)),
                        );
                    }
                });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Total Points: {} |", self.total as usize));

                if let Some(range) = &self.selection {
                    let selected_data: Vec<_> = self
                        .data
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| range.contains(i))
                        .map(|(_, (_, count))| *count)
                        .collect();

                    ui.label(format!("Selected bars: {} |", selected_data.len()));
                    if let (Some(&min), Some(&max)) =
                        (selected_data.iter().min(), selected_data.iter().max())
                    {
                        ui.label(format!("Min count: {} |", min));
                        ui.label(format!("Max count: {} |", max));
                        ui.label(format!(
                            "Total in selection: {}",
                            selected_data.iter().copied().sum::<usize>()
                        ));
                    }
                }
            });
        });
    }
}
