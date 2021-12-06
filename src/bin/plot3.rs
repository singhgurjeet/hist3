#[macro_use]
extern crate clap;

use atty::Stream;
use druid::{
    AppLauncher, Command, DelegateCtx, Env, ExtEventSink, Handled, LocalizedString, Selector, Size,
    Target, Widget, WindowDesc,
};
use hist3::data::InputSource;
use hist3::plot_widget;
use hist3::plot_widget::AppState;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::{io, thread};
use druid::widget::Align;

const NEW_DATA: Selector<f64> = Selector::new("new_data");

struct Delegate {
    // eventsink: ExtEventSink,
}

impl Delegate {
    fn new() -> Self {
        Delegate { }
    }
}

impl druid::AppDelegate<AppState> for Delegate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppState,
        _env: &Env,
    ) -> Handled {
        if let Some(val) = cmd.get(NEW_DATA) {
            if data.vals.len() == 0 {
                data.min = *val;
                data.max = *val;
            }
            data.vals.push(*val);
            if *val < data.min {
                data.min = *val;
            } else if *val > data.max {
                data.max = *val;
            }
        }

        Handled::Yes
    }
}

fn build_main_window() -> impl Widget<AppState> {
    Align::centered(plot_widget::Plot{})
}

pub fn main() {
    let matches = clap_app!(myapp =>
        (version: "0.1")
        (about: "Simple line plot. Data must either be piped in or given as an argument")
        (@arg INPUT: "Sets the input file to use")
    )
    .get_matches();
    let input = if !atty::is(Stream::Stdin) {
        InputSource::Stdin
    } else {
        let file_name = matches.value_of("INPUT").expect("No input").to_owned();
        if !Path::new(&file_name).exists() {
            panic!("File does not exist");
        }
        InputSource::FileName(file_name)
    };
    let main_window = WindowDesc::new(build_main_window)
        .title(LocalizedString::new("Plot"))
        .window_size(Size {
            width: 800.0,
            height: 600.0,
        });
    let app = AppLauncher::with_window(main_window);
    let delegate = Delegate::new();
    let sink = app.get_external_handle();
    // let sink = delegate.eventsink.clone();
    thread::spawn(move || {
        stream_numbers(input, sink);
    });

    app.delegate(delegate)
        // .use_simple_logger()
        .launch(AppState::default())
        .expect("launch failed");
}

pub fn stream_numbers(input: InputSource, sink: ExtEventSink) {
    let mut line = String::new();

    match input {
        InputSource::Stdin => {
            let reader = std::io::stdin();
            loop {
                match reader.read_line(&mut line) {
                    Ok(bytes_read) => {
                        if bytes_read == 0 {
                            break;
                        }
                        process_line(&sink, &mut line);
                    }
                    Err(_) => {}
                }
            }
        }
        InputSource::FileName(file_name) => {
            let file = File::open(file_name).unwrap();
            let mut reader = io::BufReader::new(file);
            loop {
                match reader.read_line(&mut line) {
                    Ok(bytes_read) => {
                        if bytes_read == 0 {
                            break;
                        }
                        process_line(&sink, &mut line);
                    }
                    Err(_) => {}
                }
            }
        }
    };
}

fn process_line(sink: &ExtEventSink, line: &mut String) {
    if let Ok(val) = line.trim().parse::<f64>() {
        sink.submit_command(NEW_DATA, val, Target::Auto)
            .expect("command failed to submit");
    }
    line.clear();
}
