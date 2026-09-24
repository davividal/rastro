//! Finding the document in what a CLI tool wrote.
//!
//! **The Erlang runtime can speak before the answer does.** Measured on a GitHub runner with
//! RabbitMQ 3.12.1: every read of the facet failed because `rabbitmqctl status --formatter
//! json` had written this to stdout ahead of its document,
//!
//! ```text
//! =ERROR REPORT==== 23-Sep-2026::14:03:05.184639 ===
//! file:path_eval(["/var/lib/rabbitmq","/home/runner/.config/erlang"],".erlang"): permission denied
//! ```
//!
//! and a parse that started at the first byte saw `=` where it wanted `{`.
//!
//! **It could not be reproduced in a container**, which is what settles the design rather
//! than a taste for leniency: the same version on Ubuntu 24.04 answers at byte 0, with a
//! cleared environment and with `HOME` pointing at somewhere unreadable. Whatever the runner
//! does differently, the VM's report is a property of the host rather than of the version, so
//! rastro cannot spot it in advance and must be able to read past it.
//!
//! **Past it, and no further.** The document starts at the first line beginning with `{` or
//! `[`, so a preamble is skipped whole lines at a time rather than by hunting for a bracket
//! that an Erlang term in a report could perfectly well contain — and that report's own
//! second line begins with `file:path_eval([`, which is exactly the hunt going wrong. Output
//! with no such line is handed back unchanged, so the caller's own error carries what the
//! tool actually said.
//!
//! **Both openings, because both are documents.** `status` and `export_definitions` answer
//! with an object; every `list_*` command answers with an array, and a reader that knew only
//! about `{` would skip a whole answer looking for one.

/// The opening of a JSON document, whichever kind it is.
const OPENINGS: [char; 2] = ['{', '['];

/// The JSON document inside `output`, or all of it when none is recognisable.
pub fn document_in(output: &str) -> &str {
    let mut offset = 0;

    for line in output.lines() {
        if line.starts_with(OPENINGS) {
            return &output[offset..];
        }

        // `lines()` drops the separator, and a file ending without one would otherwise put
        // the offset past the end. Saturating is not a guard against that: it is the record
        // of it, since the loop stops at the last line either way.
        offset = offset.saturating_add(line.len() + 1).min(output.len());
    }

    output
}
