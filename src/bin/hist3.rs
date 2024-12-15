#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

extern crate egui_plot;

use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui_plot::{Bar, BarChart, Legend, Plot};
use hist3::data;
use hist3::data::InputSource;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::path::Path;

mod colors {
    use eframe::egui::Color32;

    pub const SELECTED_BAR_COLOR: Color32 = Color32::from_rgb(255, 165, 0);
    pub const PERCENTILE_25_COLOR: Color32 = Color32::from_rgb(77, 77, 255);
    pub const PERCENTILE_50_COLOR: Color32 = Color32::from_rgb(77, 255, 77);
    pub const PERCENTILE_75_COLOR: Color32 = Color32::from_rgb(255, 77, 77);
    pub const DEFAULT_BAR_COLOR: Color32 = Color32::from_rgb(75, 75, 75);
}

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

    let plot = HistApp::new(labels_and_counts, p_25, p_50, p_75, total, range, args.bins);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0]) // Wider default window
            .with_min_inner_size([400.0, 300.0]), // Set minimum size
        ..Default::default()
    };
    eframe::run_native(title.as_str(), options, Box::new(|_| Ok(Box::new(plot))))
}

struct HistApp {
    data: Vec<(String, usize)>,
    p_25: Option<(f64, f64)>,
    p_50: Option<(f64, f64)>,
    p_75: Option<(f64, f64)>,
    total: f64,
    range: f64,
    num_bins: usize,
    grid: bool,
    axes: bool,
    selection: Option<HashSet<usize>>,
    drag_start: Option<egui_plot::PlotPoint>,
    drag_end: Option<egui_plot::PlotPoint>,
}

impl HistApp {
    fn new(
        data: Vec<(String, usize)>,
        p_25: Option<(f64, f64)>,
        p_50: Option<(f64, f64)>,
        p_75: Option<(f64, f64)>,
        total: f64,
        range: f64,
        num_bins: usize,
    ) -> Self {
        HistApp {
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
        let bar_x = self.data[bar_idx]
            .0
            .parse::<f64>()
            .unwrap_or(bar_idx as f64);
        let bar_y = self.data[bar_idx].1 as f64;

        let width = self.range / self.num_bins as f64;
        let half_width = width / 2.0;

        let rect_x_min = start.x.min(end.x);
        let rect_x_max = start.x.max(end.x);
        let rect_y_min = start.y.min(end.y);
        let rect_y_max = start.y.max(end.y);

        let bar_rect_x_min = bar_x - half_width;
        let bar_rect_x_max = bar_x + half_width;

        rect_x_max >= bar_rect_x_min
            && rect_x_min <= bar_rect_x_max
            && rect_y_max >= 0.0
            && rect_y_min <= bar_y
    }
}

impl eframe::App for HistApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let width = if self.p_25.is_some() {
            self.range / self.num_bins as f64
        } else {
            1.0
        };
        let min_x = self.data.first().unwrap().0.parse::<f64>().unwrap_or(0.0);
        let max_x = self
            .data
            .last()
            .unwrap()
            .0
            .parse::<f64>()
            .unwrap_or(self.data.len() as f64);
        let max_y = *self.data.iter().map(|(_, c)| c).max().unwrap() as f64;
        let chart = BarChart::new(
            self.data
                .iter()
                .enumerate()
                .map(|(i, (label, count))| {
                    let mut bar = if self.p_25.is_some() {
                        Bar::new(label.parse::<f64>().unwrap(), *count as f64)
                            .width(width)
                            .name(label)
                            .fill(colors::DEFAULT_BAR_COLOR)
                    } else {
                        Bar::new(i as f64, *count as f64)
                            .width(1.0)
                            .name(label)
                            .fill(colors::DEFAULT_BAR_COLOR)
                    };

                    if let Some(selected_indices) = &self.selection {
                        if selected_indices.contains(&i) {
                            bar = bar.fill(colors::SELECTED_BAR_COLOR);
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
                        let pointer = egui_plot::PlotPoint {
                            x: pointer.x.clamp(min_x, max_x),
                            y: pointer.y.clamp(0.0, max_y),
                        };
                        if plot_ui.ctx().input(|i| i.pointer.primary_pressed()) {
                            self.drag_start = Some(pointer);
                            self.drag_end = Some(pointer);
                        } else if plot_ui.ctx().input(|i| i.pointer.primary_down()) {
                            self.drag_end = Some(pointer);
                        } else if plot_ui.ctx().input(|i| i.pointer.primary_released()) {
                            if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                                let selected_bars: HashSet<usize> = (0..self.data.len())
                                    .filter(|&i| self.is_bar_in_rect(i, &start, &end))
                                    .collect();

                                if !selected_bars.is_empty() {
                                    self.selection = Some(selected_bars);
                                } else {
                                    self.selection = None;
                                }
                            }
                            self.drag_start = None;
                            self.drag_end = None;
                        }
                    }

                    plot_ui.bar_chart(chart.width(width * 0.98));

                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                        plot_ui.polygon(egui_plot::Polygon::new(egui_plot::PlotPoints::from_iter(
                            vec![
                                [start.x, start.y],
                                [end.x, start.y],
                                [end.x, end.y],
                                [start.x, end.y],
                            ],
                        )));
                    }

                    if let Some((_, x)) = self.p_25 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(colors::PERCENTILE_25_COLOR)
                                .name(format!("25 ptile: {:.4}", x)),
                        );
                    }
                    if let Some((_, x)) = self.p_50 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(colors::PERCENTILE_50_COLOR)
                                .name(format!("50 ptile: {:.4}", x)),
                        );
                    }
                    if let Some((_, x)) = self.p_75 {
                        plot_ui.vline(
                            egui_plot::VLine::new(x)
                                .color(colors::PERCENTILE_75_COLOR)
                                .name(format!("75 ptile: {:.4}", x)),
                        );
                    }
                });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Total Points: {} |", self.total as usize));

                if let Some(selected_indices) = &self.selection {
                    let selected_data: Vec<_> = self
                        .data
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| selected_indices.contains(i))
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
