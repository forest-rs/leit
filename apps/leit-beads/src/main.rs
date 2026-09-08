// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read-only search across Beads JSONL snapshots.

mod cli;
mod load;
mod model;
mod output;
mod search;
mod text;

use std::{
    io::{self, Write},
    process::ExitCode,
    time::Instant,
};

fn run() -> Result<String, String> {
    let Some(options) = cli::Options::parse(std::env::args_os().skip(1))? else {
        return Ok(cli::HELP.to_owned());
    };
    let start = Instant::now();
    let (sources, issues) = load::load(&options.sources)?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    let outcome = search::run(&issues, &options)?;
    output::render(&sources, &issues, &options, &outcome, load_ms)
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => match io::stdout().lock().write_all(output.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("leit-beads: {error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("leit-beads: {}", text::plain(&error));
            ExitCode::from(2)
        }
    }
}
