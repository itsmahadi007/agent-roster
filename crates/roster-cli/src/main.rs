use clap::Parser;

#[derive(Parser)]
#[command(name = "roster", version, about = "Name your AI coding accounts and use them from any terminal")]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
