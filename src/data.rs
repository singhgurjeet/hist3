use std::fs::File;
use std::io::{self, BufRead};
use itertools::Itertools;
use crate::InputSource;
use std::collections::HashMap;

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
        .count() > (vals.len() / 2)
}

fn histogram_from_categories(vals: &Vec<String>) -> (Vec<(String, usize)>, Option<f64>, Option<f64>, Option<f64>, f64) {
    let ret: Vec<(String, usize)> = vals.iter()
        .sorted()
        .group_by(|e| (**e).to_owned())
        .into_iter()
        .map(|(k, group_k)| (k, group_k.count()))
        .sorted_by(|(_, i), (_, j)| i.cmp(j))
        .collect();
    let total = ret.iter().fold(0.0, |t, (_s, x)| t + *x as f64);
    (ret, None, None, None, total)
}

fn histogram_from_numbers(vals: &Vec<String>, num_bars: &usize) -> (Vec<(String, usize)>, Option<f64>, Option<f64>, Option<f64>, f64) {
    let sorted_nums: Vec<f64> = vals.iter()
        .filter(|x| x.len() > 0)
        .map(|x| x.parse::<f64>())
        .filter(|x| x.is_ok())
        .map(|x| x.unwrap())
        .sorted_by(|x, y| compare_f64(x,y))
        .collect::<Vec<f64>>();
    let min = *sorted_nums.first().unwrap();
    let max = *sorted_nums.last().unwrap();
    let range = max - min;
    let delta = range / (*num_bars as f64);
    let len_25 = ((sorted_nums.len() as f64) * 0.25) as usize;
    let existing_counts: HashMap<usize, usize> = sorted_nums.iter().map(|x| ((*x - min) / delta) as usize)
        .group_by(|e| *e)
        .into_iter()
        .map(|(k, group_k)| (k, group_k.count()))
        .collect();
    let total = existing_counts.iter().fold(0.0, |t, (_s, x)| t + *x as f64);
    (
        (0..*num_bars).map(|i| (i as usize, existing_counts.get(&(i as usize)).unwrap_or(&(0 as usize))))
            .map(|(i, val)| (format!("{:.2}", min + (i as f64) * delta + 0.5 as f64), *val))
            .collect::<Vec<(String, usize)>>(),
        Some((sorted_nums[len_25] - min)/ range),
        Some((sorted_nums[len_25 * 2] - min)/ range),
        Some((sorted_nums[len_25 * 3] - min)/ range),
        total
    )
}

pub fn compute_histogram(num_bins: usize, input: InputSource) -> (Vec<(String, usize)>, Option<f64>, Option<f64>, Option<f64>, f64) {
    let max_num_lines = 10_000_000;

    let vals = match input {
        InputSource::Stdin => read_from_stdin(max_num_lines),
        InputSource::FileName(file_name) => read_from_file(&file_name, max_num_lines)
    };

    let mostly_string = is_mostly_strings(&vals);

    let num_uniques = vals.iter().unique().count();

    if mostly_string || num_uniques < num_bins {
        histogram_from_categories(&vals)
    } else {
        histogram_from_numbers(&vals, &num_bins)
    }
}