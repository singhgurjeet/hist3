#[macro_use]
extern crate clap;

mod data;
mod histogram_widget;
mod styles;

use atty::Stream;
use druid::widget::prelude::*;
use druid::widget::{Align, Either, Label};
use druid::{
    AppDelegate, AppLauncher, Command, Data, DelegateCtx, ExtEventSink, LocalizedString, Selector,
    Target, WindowDesc,
};
use std::thread;
use std::path::Path;

const LOAD_DATA: Selector<(InputSource, usize)> = Selector::new("load_data");
const LOADED: Selector<AppState> = Selector::new("loaded_data");

#[derive(Clone)]
pub enum InputSource {
    FileName(String),
    Stdin,
}

fn wrapped_load_data(sink: ExtEventSink, input: InputSource, num_bins: usize) {
    thread::spawn(move || {
        let (labels_and_counts, p_25, p_50, p_75, total) = data::compute_histogram(num_bins, input);

        sink.submit_command(
            LOADED,
            AppState {
                loaded: true,
                labels_and_counts,
                p_25,
                p_50,
                p_75,
                total,
                highlight: None,
            },
            None,
        )
        .expect("command failed to submit");
    });
}

#[derive(Clone, Default, Debug)]
struct AppState {
    loaded: bool,
    labels_and_counts: Vec<(String, usize)>,
    p_25: Option<f64>,
    p_50: Option<f64>,
    p_75: Option<f64>,
    total: f64,
    highlight: Option<usize>,
}

impl Data for AppState {
    fn same(&self, other: &Self) -> bool {
        self.loaded.eq(&other.loaded)
            && self.p_25.eq(&other.p_25)
            && self.p_50.eq(&other.p_50)
            && self.p_75.eq(&other.p_75)
            && self.total.eq(&other.total)
            && self.highlight.eq(&other.highlight)
            && self
                .labels_and_counts
                .iter()
                .zip(other.labels_and_counts.iter())
                .all(|((s, i), (os, oi))| s.eq(os) && i.eq(oi))
    }
}

struct Delegate {
    eventsink: ExtEventSink,
}

impl Delegate {
    fn new(eventsink: ExtEventSink, input: InputSource, num_bins: usize) -> Self {
        eventsink
            .submit_command(LOAD_DATA, (input, num_bins), None)
            .expect("Could not load data");
        Delegate { eventsink }
    }
}

impl AppDelegate<AppState> for Delegate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppState,
        _env: &Env,
    ) -> bool {
        if let Some((input, num_bins)) = cmd.get(LOAD_DATA) {
            wrapped_load_data(self.eventsink.clone(), (*input).clone(), *num_bins);
        }
        if let Some(histogram_data) = cmd.get(LOADED) {
            data.loaded = true;
            data.labels_and_counts = (*histogram_data.labels_and_counts.to_owned()).to_vec();
            data.p_25 = histogram_data.p_25;
            data.p_50 = histogram_data.p_50;
            data.p_75 = histogram_data.p_75;
            data.total = histogram_data.total;
            data.highlight = None;
        }
        true
    }
}

fn build_main_window() -> impl Widget<AppState> {
    let text = LocalizedString::new("Loading...");
    let loading_text = Label::new(text);
    let histogram = histogram_widget::Histogram {};

    let either = Either::new(
        |data: &AppState, _env| !data.loaded,
        loading_text,
        histogram,
    );

    Align::centered(either)
}

pub fn main() {
    let matches = clap_app!(myapp =>
                (version: "0.1")
                (about: "Plot the distribution of input. Data must either be piped in or given as an argument")
                (@arg BINS: -b --bins +takes_value "The number of bins in the histogram")
                (@arg INPUT: "Sets the input file to use")
            ).get_matches();
    let input = if !atty::is(Stream::Stdin) {
        InputSource::Stdin
    } else {
        let file_name = matches.value_of("INPUT").expect("No input").to_owned();
        if !Path::new(&file_name).exists() {
            panic!("File does not exist");
        }
        InputSource::FileName(file_name)
    };
    let num_bins = matches
        .value_of("BINS")
        .unwrap_or("20")
        .parse::<usize>()
        .unwrap();

    let main_window = WindowDesc::new(build_main_window)
        .title(LocalizedString::new("Histogram"))
        .window_size(Size {
            width: 800.0,
            height: 600.0,
        });
    let app = AppLauncher::with_window(main_window);
    let delegate = Delegate::new(app.get_external_handle(), input, num_bins);

    app.delegate(delegate)
        // .use_simple_logger()
        .launch(AppState::default())
        .expect("launch failed");
}
