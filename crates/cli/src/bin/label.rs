//! `label` — replaces `label.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "label", about = "Render label SVGs for part IDs")]
struct Args {
    /// Canonical part IDs to render.
    #[arg(required = true)]
    ids: Vec<String>,

    /// Tape height in mm (per ADR-021 default override).
    #[arg(long)]
    size: Option<f64>,

    /// Use Micro QR (M4) instead of Standard QR.
    #[arg(long, default_value_t = false)]
    micro: bool,
}

fn main() {
    let _args = Args::parse();
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
