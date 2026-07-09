mod app;
mod storage;
mod syntax;
mod theme;

use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "dopepad", version, about = "Fast minimalist GUI notepad")]
pub struct Cli {
    /// Open or create today's daily note.
    #[arg(long, conflicts_with_all = ["new", "daemon"])]
    pub daily: bool,

    /// Create a new loose note.
    #[arg(long, conflicts_with_all = ["daily", "daemon"])]
    pub new: bool,

    /// Warm the process in the background (no window).
    #[arg(long, conflicts_with_all = ["daily", "new"])]
    pub daemon: bool,

    /// Open a file (.dpad, .txt, .md, …).
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum LaunchAction {
    Daemon,
    Daily,
    New,
    File(PathBuf),
}

impl LaunchAction {
    pub fn from_cli(cli: &Cli) -> Self {
        if cli.daemon {
            LaunchAction::Daemon
        } else if cli.new {
            LaunchAction::New
        } else if let Some(ref path) = cli.file {
            LaunchAction::File(path.clone())
        } else {
            LaunchAction::Daily
        }
    }
}

fn main() {
    // clap handles --help / --version before GTK starts.
    let _cli = Cli::parse();

    let code = app::run();
    if code != gtk::glib::ExitCode::SUCCESS {
        std::process::exit(code.value());
    }
}
