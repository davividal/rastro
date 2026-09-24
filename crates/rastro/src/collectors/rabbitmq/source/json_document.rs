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

//! **Which field, not which byte.** Every read here goes through one deserialiser that names
//! the field it failed at. A collector reports to an operator who no longer has the document —
//! it was several megabytes, it was never written down, and the node has moved on since — so
//! `at line 1 column 4889` names nothing anybody can act on. Measured on a live 3.10.8 box,
//! where exactly that message was the whole of what a failed facet had to say.

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

/// What `serde_path_to_error` prints when the document itself is the failure rather than a
/// field in it.
const WHOLE_DOCUMENT: &str = ".";

/// The document in `output`, read into `T`, with the field that failed named.
///
/// The error is the failure's own words with the field's path in front of them, which the
/// caller puts after its own account of which command it ran. A document that is not JSON at
/// all fails at the root and is reported unprefixed, since there is no field to name.
pub fn read_document<T: serde::de::DeserializeOwned>(output: &str) -> Result<T, String> {
    let mut deserializer = serde_json::Deserializer::from_str(document_in(output));

    serde_path_to_error::deserialize(&mut deserializer).map_err(|failure| {
        let field = failure.path().to_string();
        let cause = failure.into_inner();

        if field == WHOLE_DOCUMENT {
            return cause.to_string();
        }

        format!("{field}: {cause}")
    })
}
