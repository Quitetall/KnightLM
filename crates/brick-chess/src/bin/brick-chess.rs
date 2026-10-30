use std::path::PathBuf;

use clap::{Parser, Subcommand};
use brick_chess::{Command, Event, Widget};

#[derive(Parser)]
#[command(name = "brick-chess", version, about = "BrickChess widget: JSON command/event protocol + renderer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// JSON-lines protocol on stdin/stdout (default).
    Repl,
    /// Render a position to SVG.
    Render {
        #[arg(long)]
        fen: String,
        #[arg(long, default_value = "green")]
        theme: String,
        #[arg(long, default_value = "chessnut")]
        pieces: String,
        #[arg(long)]
        flip: bool,
    },
    /// Replay a PGN file, optionally rendering the final position.
    Play {
        #[arg(long)]
        pgn: PathBuf,
        #[arg(long)]
        render: Option<PathBuf>,
        #[arg(long, default_value = "green")]
        theme: String,
        #[arg(long, default_value = "chessnut")]
        pieces: String,
    },
}

fn first_error(events: &[Event]) -> Option<String> {
    events.iter().find_map(|e| match e {
        Event::Error { code, detail } => Some(format!("{code}: {detail}")),
        _ => None,
    })
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Cmd::Repl) {
        Cmd::Repl => {
            brick_chess::protocol::run(std::io::stdin().lock(), std::io::stdout().lock())?;
        }
        Cmd::Render { fen, theme, pieces, flip } => {
            let mut w = Widget::from_fen(&fen)?;
            if flip {
                w.apply(Command::Flip);
            }
            w.apply(Command::SetTheme { theme });
            w.apply(Command::SetPieces { pieces });
            print!("{}", w.export("svg")?);
        }
        Cmd::Play { pgn, render, theme, pieces } => {
            let text = std::fs::read_to_string(&pgn)?;
            let mut w = Widget::new();
            let events = w.apply(Command::LoadPgn { pgn: text });
            if let Some(e) = first_error(&events) {
                anyhow::bail!("{e}");
            }
            w.apply(Command::SetTheme { theme });
            w.apply(Command::SetPieces { pieces });
            match render {
                Some(path) => std::fs::write(&path, w.export("svg")?)?,
                None => print!("{}", w.export("pgn")?),
            }
        }
    }
    Ok(())
}
