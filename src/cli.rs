use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    RemoveSong { radio: String, song: String },
    RemoveRadio { radio: String },
}
fn main() {
    let args = Args::parse();
}
