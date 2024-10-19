use clap::{Parser, Subcommand};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    RemoveSong { radio: String, song: String },
    RemoveRadio { radio: String },
    Save,
    Shutdown,
}
#[derive(Serialize, Deserialize, Type)]
pub enum SerCommand {
    RemoveSong { radio: String, song: String },
    RemoveRadio { radio: String, _un: String },
    Save { _un1: String, _un2: String },
    Shutdown { _un1: String, _un2: String },
}
#[allow(dead_code)]
fn main() {
    let args = Args::parse();
}
