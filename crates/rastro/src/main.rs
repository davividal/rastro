//! The tool: built-in collectors, the command line, and the wiring between
//! them. Every decision worth testing lives in a crate, not here.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::error::Error;
use std::io;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::SystemTime;

use rastro::collectors::filesystem::Detail;
use rastro::config::Config;
use rastro::output::{self, Destination, Written};
use rastro::preflight;
use rastro::progress::{self, Reporting, WalkProgress};
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

/// What this run resolved before any collector saw the host.
///
/// A named type rather than a dozen locals, because these are decisions rather than working
/// values and several of them have to reach two places without being read twice: the clock and
/// the hostname because the output filename carries both, and the destination because the walk
/// leaves it out and the `invocation` facet declares it.
struct Resolved {
    config: Config,
    detail: Detail,
    destination: Destination,
    started_at: Result<i64, rastro_collector::CollectionError>,
    hostname: Result<String, String>,
    reporting: Option<Arc<Reporting>>,
    debug: bool,
}

fn run() -> Result<Written, Box<dyn Error>> {
    let invocation = cli::parse();
    let resolved = resolve(&invocation)?;

    warn_if_the_document_may_crowd_the_disk(&resolved);

    let selection = collectors::selected(
        collectors::built_in(collectors::Run {
            effective_config: collectors::effective_config(
                &resolved.config,
                invocation.view(),
                invocation.staged_binary(),
                resolved.detail,
            ),
            staged_binary: invocation.staged_binary(),
            detail: resolved.detail,
            started_at: resolved.started_at.clone(),
            hostname: resolved.hostname.clone(),
            output: walked_output(&resolved.destination),
            narrowed: collectors::Narrowed {
                metadata_only: resolved.config.walk_metadata_only().to_vec(),
                churns: resolved.config.walk_churns().to_vec(),
                sealed: resolved.config.walk_sealed().to_vec(),
            },
            progress: resolved
                .reporting
                .clone()
                .map(|sink| sink as Arc<dyn WalkProgress>),
        }),
        &resolved.config,
    )?;

    for name in selection.excluded() {
        say(
            &resolved,
            &format!("{name} excluded by config, so it is not in this fingerprint"),
        );
    }

    let fingerprint = match &resolved.reporting {
        Some(sink) => fingerprint_host::run_reporting(selection.running(), sink.as_ref())?,
        None => fingerprint_host::run(selection.running())?,
    };

    let written = output::write(
        &resolved.destination,
        &fingerprint,
        invocation.view(),
        invocation.force(),
    )?;

    report(&resolved, &written)?;

    Ok(written)
}

/// Everything the run decides for itself, before it reads anything but the clock.
fn resolve(invocation: &cli::Cli) -> Result<Resolved, Box<dyn Error>> {
    let config = match invocation.config_path() {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };

    // One sink, watching both the collectors and the walk inside one of them. Present when
    // either half was asked for, and built before anything runs so the total it reports is the
    // whole run rather than the part after somebody thought to start a clock.
    let live = invocation.show_progress(progress::stderr_is_a_terminal());
    let debug = invocation.debug();

    // Both read once, because the output filename carries the same instant and the same
    // hostname the document does, and two reads could disagree.
    let started_at = collectors::seconds_since_epoch(SystemTime::now());
    let hostname = collectors::read_hostname();

    Ok(Resolved {
        destination: Destination::resolve(
            invocation.output(),
            hostname.as_deref().ok(),
            started_at.as_ref().copied().unwrap_or_default(),
        ),
        detail: match invocation.full_detail() {
            true => Detail::Full,
            false => Detail::Summary,
        },
        config,
        started_at,
        hostname,
        reporting: (live || debug).then(|| Arc::new(Reporting::new(live))),
        debug,
    })
}

/// The document's own path as the walk will meet it, so the walk can leave it out.
///
/// `None` for stdout, where there is no file to leave out.
fn walked_output(destination: &Destination) -> Option<std::path::PathBuf> {
    match destination {
        Destination::File(path) => Some(output::as_walked(path)),
        Destination::Stdout => None,
    }
}

/// Before the walk, because the point is to change the operator's mind while they still have one
/// to change. A warning, never a limit: a budget an operator must tune presupposes they already
/// investigated the box, which is the work rastro is here to do.
fn warn_if_the_document_may_crowd_the_disk(resolved: &Resolved) {
    let Destination::File(path) = &resolved.destination else {
        return;
    };

    if let Some(estimate) = preflight::estimate()
        && let Some(concern) = preflight::concern(estimate, preflight::free_bytes_at(path))
    {
        say(resolved, &concern);
    }
}

/// One line to the operator, without a live counter half-overwriting it.
fn say(resolved: &Resolved, message: &str) {
    if let Some(sink) = &resolved.reporting {
        sink.clear();
    }

    eprintln!("rastro: {message}");
}

/// The `--debug` report, once the document is safely written.
fn report(resolved: &Resolved, written: &Written) -> Result<(), Box<dyn Error>> {
    let Some(sink) = &resolved.reporting else {
        return Ok(());
    };

    // The counter shares the line the report is about to use.
    sink.clear();

    if resolved.debug {
        let wrote = match &written.destination {
            Destination::File(path) => format!("{} ({} bytes)", path.display(), written.bytes),
            Destination::Stdout => format!("stdout ({} bytes)", written.bytes),
        };
        sink.report(&mut io::stderr(), &wrote)?;
    }

    Ok(())
}
