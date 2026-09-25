//! The Deltu executable: CLI dispatch (spec 09). All command behavior
//! lives in `src/cli.rs`; this is the entry point only.

use clap::Parser as _;

fn main() {
    let cli = deltu::cli::Cli::parse();
    let code = deltu::cli::execute(cli.command);
    std::process::exit(code);
}
