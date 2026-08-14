//! The tool: built-in collectors, the command line, and the wiring between
//! them. Every decision worth testing lives in a crate, not here.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::process::ExitCode;

use rastro::{cli, collectors};
use rastro_collector::fingerprint_host;
use rastro_fingerprint::json;

fn main() -> ExitCode {
    let invocation = cli::parse();
    let collectors = collectors::built_in();

    match fingerprint_host::run(&collectors) {
        Ok(fingerprint) => {
            print!(
                "{}",
                json::to_canonical_json(&fingerprint, invocation.view())
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rastro: {error}");
            ExitCode::FAILURE
        }
    }
}
