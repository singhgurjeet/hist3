pub mod data;

use regex::Regex;

pub static NUMERIC_REGEX: once_cell::sync::Lazy<Regex> =
    once_cell::sync::Lazy::new(|| Regex::new(r"[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?").unwrap());
