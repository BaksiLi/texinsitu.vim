//! The resident TeXinsitu renderer: one JSON request per line in, one reply
//! per line out. A request is `{id, lines, filetype, context, cells}`; the reply is
//! `{id, formulas}`, each formula its position plus its text layout.
//!
//! Layouts depend only on a formula's source, whether it is displayed and the
//! definitions in scope, so each is kept until it changes and an edit lays out
//! only the formulas it touched. A formula that cannot be laid out stays
//! source: it is left out of the reply.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use texinsitu::{Cells, Kernel, Rendered};

#[derive(Deserialize)]
struct Request {
    id: Value,
    lines: Vec<String>,
    filetype: String,
    #[serde(default)]
    context: String,
    #[serde(default)]
    cells: Cells,
}

#[derive(Serialize)]
struct Reply {
    id: Value,
    formulas: Vec<Rendered>,
}

fn main() {
    let mut kernel = Kernel::default();
    let stdout = io::stdout();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let formulas = kernel.document(
                    &request.lines.join("\n"),
                    &request.filetype,
                    &request.context,
                    request.cells,
                );
                let mut out = stdout.lock();
                let reply = Reply { id: request.id, formulas };
                if serde_json::to_writer(&mut out, &reply).is_err()
                    || writeln!(out).is_err()
                    || out.flush().is_err()
                {
                    break; // The editor has gone.
                }
            }
            Err(error) => eprintln!("request: {error}"),
        }
    }
}
