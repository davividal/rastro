//! The tool: built-in collectors, the command line, and the wiring between
//! them. Every decision worth testing lives in a crate, not here.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::error::Error;
use std::process::ExitCode;
use std::time::SystemTime;

use rastro::collectors::filesystem::Detail;
use rastro::config::Config;
use rastro::output::{self, Destination, Written};
use rastro::{cli, collectors};
use rastro_collector::fingerprint_host;

fn main() -> ExitCode {
    match run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rastro: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<Written, Box<dyn Error>> {
    let invocation = cli::parse();

    let config = match invocation.config_path() {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };

    let detail = match invocation.full_detail() {
        true => Detail::Full,
        false => Detail::Summary,
    };

    let effective = collectors::effective_config(
        &config,
        invocation.view(),
        invocation.staged_binary(),
        detail,
    );
    // Both read once, here, because the output filename carries the same instant and the
    // same hostname as the document does. Two reads could disagree.
    let started_at = collectors::seconds_since_epoch(SystemTime::now());
    let hostname = collectors::read_hostname();
    let destination = Destination::resolve(
        invocation.output(),
        hostname.as_deref().ok(),
        started_at.as_ref().copied().unwrap_or_default(),
    );

    let run = collectors::Run {
        effective_config: effective,
        staged_binary: invocation.staged_binary(),
        detail,
        started_at,
        hostname,
    };
    let selection = collectors::selected(collectors::built_in(run), &config)?;
    for name in selection.excluded() {
        eprintln!("rastro: {name} excluded by config, so it is not in this fingerprint");
    }

    let fingerprint = fingerprint_host::run(selection.running())?;

    Ok(output::write(
        &destination,
        &fingerprint,
        invocation.view(),
        invocation.force(),
    )?)
}
