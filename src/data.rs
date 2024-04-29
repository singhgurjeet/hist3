use itertools::Itertools;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};

#[derive(Clone)]
pub enum InputSource {
    FileName(String),
    Stdin,
}

fn compare_f64(x: &f64, y: &f64) -> std::cmp::Ordering {
    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
}

fn read_from_stdin(max_num_lines: usize) -> Vec<String> {
    let mut vals: Vec<String> = Vec::new();
    for line in std::io::stdin().lock().lines().take(max_num_lines) {
        vals.push(line.unwrap().trim().to_owned());
    }
    vals
}

fn read_from_file(file_name: &String, max_num_lines: usize) -> Vec<String> {
    let mut vals: Vec<String> = Vec::new();
    let file = File::open(file_name).unwrap();
    for line in io::BufReader::new(file).lines().take(max_num_lines) {
        vals.push(line.unwrap().trim().to_owned());
    }
    vals
}

fn is_mostly_strings(vals: &Vec<String>) -> bool {
    vals.iter()
        .filter(|x| x.len() > 0)
        .map(|x| x.parse::<f64>())
        .filter(|x| x.is_err())
        .count()
        > (vals.len() / 2)
}

fn histogram_from_categories(
    vals: &Vec<String>,
) -> (
    Vec<(String, usize)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    f64,
) {
    let ret: Vec<(String, usize)> = vals
        .iter()
        .sorted()
        .group_by(|e| (**e).to_owned())
        .into_iter()
        .map(|(k, group_k)| (k, group_k.count()))
        .sorted_by(|(_, i), (_, j)| i.cmp(j))
        .collect();
    let total = ret.iter().fold(0.0, |t, (_s, x)| t + *x as f64);
    (ret, None, None, None, total)
}

/// Generates a histogram from a vector of numerical string values.
///
/// This function takes a vector of strings which are expected to be parseable as floating-point
/// numbers and a reference to the desired number of bars (bins) for the histogram. It returns
/// a tuple containing the histogram as a vector of tuples where each tuple consists of a string
/// representation of the bin's midpoint and the count of values in that bin, and three `Option`
/// tuples representing the 25th, 50th, and 75th percentiles, respectively, if they can be computed.
/// It also returns the total count of all values as a `f64`.
///
/// # Arguments
///
/// * `vals` - A reference to a vector of strings to be parsed into floating-point numbers.
/// * `num_bars` - A reference to the number of bars (bins) the histogram should have.
///
/// # Returns
///
/// A tuple containing:
/// - A vector of tuples, where each tuple contains a string representation of the bin's midpoint
///   and the count of values in that bin.
/// - An `Option<(f64, f64)>` for the 25th percentile, where the first `f64` is the normalized
///   position of the percentile, and the second `f64` is the value at the 25th percentile.
/// - An `Option<(f64, f64)>` for the 50th percentile (median), similar to the 25th percentile.
/// - An `Option<(f64, f64)>` for the 75th percentile, similar to the 25th percentile.
/// - The total count of all values as a `f64`.
///
/// # Panics
///
/// This function panics if the input vector `vals` contains no parseable floating-point numbers
/// or if the first or last element cannot be parsed into a `f64`.
fn histogram_from_numbers(
    vals: &Vec<String>,
    num_bars: &usize,
) -> (
    Vec<(String, usize)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    f64,
) {
    let sorted_nums = vals
        .iter()
        .filter(|x| x.len() > 0)
        .filter_map(|x| x.parse::<f64>().ok())
        .sorted_by(|x, y| compare_f64(x, y))
        .collect::<Vec<_>>();
    let min = *sorted_nums.first().unwrap();
    let max = *sorted_nums.last().unwrap();
    let range = max - min;
    let delta = range / (*num_bars as f64);
    let len_25 = ((sorted_nums.len() as f64) * 0.25) as usize;
    let existing_counts: HashMap<usize, usize> = sorted_nums
        .iter()
        .map(|x| ((*x - min) / delta) as usize)
        .group_by(|e| *e)
        .into_iter()
        .map(|(k, group_k)| (k, group_k.count()))
        .collect();
    let total = existing_counts.iter().fold(0.0, |t, (_, x)| t + *x as f64);
    (
        (0..*num_bars)
            .map(|i| {
                (
                    format!("{:.2}", min + (i as f64) * delta + 0.5 as f64),
                    *existing_counts.get(&(i as usize)).unwrap_or(&(0 as usize)),
                )
            })
            .collect::<Vec<(String, usize)>>(),
        Some(((sorted_nums[len_25] - min) / range, sorted_nums[len_25])),
        Some((
            (sorted_nums[len_25 * 2] - min) / range,
            sorted_nums[len_25 * 2],
        )),
        Some((
            (sorted_nums[len_25 * 3] - min) / range,
            sorted_nums[len_25 * 3],
        )),
        total,
    )
}

pub fn compute_histogram(
    num_bins: usize,
    input: InputSource,
) -> (
    Vec<(String, usize)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    f64,
) {
    let max_num_lines = 10_000_000;

    let vals = match input {
        InputSource::Stdin => read_from_stdin(max_num_lines),
        InputSource::FileName(file_name) => read_from_file(&file_name, max_num_lines),
    };

    let mostly_string = is_mostly_strings(&vals);

    let num_uniques = vals.iter().unique().count();

    if mostly_string || num_uniques < num_bins {
        histogram_from_categories(&vals)
    } else {
        histogram_from_numbers(&vals, &num_bins)
    }
}
