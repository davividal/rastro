//! The tool: built-in collectors, the command line, and the wiring between
//! them. Every decision worth testing lives in a crate, not here.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::error::Error;
use std::process::ExitCode;

use rastro::config::Config;
use rastro::{cli, collectors};
use rastro_collector::fingerprint_host;
use rastro_fingerprint::json;

fn main() -> ExitCode {
    match run() {
        Ok(document) => {
            print!("{document}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rastro: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, Box<dyn Error>> {
    let invocation = cli::parse();

    let config = match invocation.config_path() {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };

    let effective = collectors::effective_config(&config, invocation.view());
    let selection = collectors::selected(&config, effective)?;
    for name in selection.excluded() {
        eprintln!("rastro: {name} excluded by config, so it is not in this fingerprint");
    }

    let fingerprint = fingerprint_host::run(selection.running())?;

    Ok(json::to_canonical_json(&fingerprint, invocation.view()))
}
