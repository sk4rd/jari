use std::path::PathBuf;

use clap::{Parser, Subcommand};
use zbus::{blocking::Connection, proxy, Result};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    RemoveSong { radio: String, song: String },
    RemoveRadio { radio: String },
    RemoveUser { sub: String },
    AddUser { sub: String },
    ListUsers,
    CountUsers,
    ListRadios,
    ListSongs { radio: String },
    ReloadPages { path: PathBuf },
    PrintState,
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
    fn remove_user(&self, sub: String) -> Result<String>;
    fn add_user(&self, sub: String) -> Result<String>;
    fn list_users(&self) -> Result<Vec<String>>;
    fn count_users(&self) -> Result<usize>;
    fn list_radios(&self) -> Result<Vec<String>>;
    fn list_songs(&self, radio: String) -> Result<Vec<String>>;
    fn reload_pages(&self, path: PathBuf) -> Result<String>;
    fn print_state(&self) -> Result<String>;
    fn save(&self) -> Result<String>;
    fn shutdown(&self) -> Result<String>;
}

#[allow(dead_code)]
fn main() {
    let args = Args::parse();
    let connection = Connection::session().unwrap();
    let client = CliListenerProxyBlocking::new(&connection).unwrap();
    let res = match args.command {
        Command::RemoveSong { radio, song } => client.remove_song(radio, song),
        Command::RemoveRadio { radio } => client.remove_radio(radio),
        Command::RemoveUser { sub } => client.remove_user(sub),
        Command::AddUser { sub } => client.add_user(sub),
        Command::ListUsers => client.list_users().map(|x| {
            x.into_iter()
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        }),
        Command::CountUsers => client.count_users().map(|x| format!("{x}")),
        Command::ListRadios => client.list_radios().map(|x| {
            x.into_iter()
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        }),
        Command::ListSongs { radio } => client.list_songs(radio).map(|x| {
            x.into_iter()
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        }),
        Command::ReloadPages { path } => client.reload_pages(path),
        Command::PrintState => client.print_state(),
        Command::Save => client.save(),
        Command::Shutdown => client.shutdown(),
    }
    .unwrap();
    println!("{res}");
}
