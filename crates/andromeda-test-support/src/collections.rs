use std::collections::{BTreeMap, BTreeSet};

pub fn virtual_files(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
        .collect()
}

pub fn path_set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}
