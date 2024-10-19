use clap::{Parser, Subcommand};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use zbus::{blocking::Connection, proxy, zvariant::Type, Result};

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

#[proxy(
    default_service = "com.github.sk4rd.jari",
    interface = "com.github.sk4rd.jari",
    default_path = "/com/github/sk4rd/jari"
)]
trait CliListener {
    fn remove_song(&self, radio: String, song: String) -> Result<String>;
    fn remove_radio(&self, radio: String) -> Result<String>;
    fn save(&self) -> Result<String>;
    fn shutdown(&mut self) -> Result<String>;
}

#[allow(dead_code)]
fn main() {
    let args = Args::parse();
    let connection = Connection::session().unwrap();
    let mut client = CliListenerProxyBlocking::new(&connection).unwrap();
    let res = match args.command {
        Command::RemoveSong { radio, song } => client.remove_song(radio, song),
        Command::RemoveRadio { radio } => client.remove_radio(radio),
        Command::Save => client.save(),
        Command::Shutdown => client.shutdown(),
    }
    .unwrap();
    println!("{res}");
}
