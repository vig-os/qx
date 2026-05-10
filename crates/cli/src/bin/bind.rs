//! `bind` — replaces `bind.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bind", about = "Bind a part ID to a serial / batch / location")]
struct Args {
    /// Canonical part ID to bind.
    id: String,

    /// Free-form binding payload (placeholder until ADR-bind lands).
    #[arg(long)]
    to: Option<String>,
}

fn main() {
    let _args = Args::parse();
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
