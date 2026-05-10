//! `mint` — replaces `mint.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold; argument parsing is sketched to lock down the
//! shape, but no business logic runs.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "mint", about = "Mint new part IDs into the registry")]
struct Args {
    /// Number of part IDs to mint.
    #[arg(short = 'n', long, default_value_t = 1)]
    count: u32,

    /// Subtype prefix (e.g. PT100, RH-1). Future ADR-012 enforced.
    #[arg(long)]
    subtype: Option<String>,
}

fn main() {
    let _args = Args::parse();
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
