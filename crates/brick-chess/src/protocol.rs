//! JSON-lines protocol: one [`Command`] per line in, [`Event`]s per line out.
//! This is the tool-call surface an agent or CLI uses.

use std::io::{BufRead, Write};

use crate::command::{Command, Event};
use crate::widget::Widget;

/// Apply one JSON command line and return the JSON event lines.
pub fn apply_line(widget: &mut Widget, line: &str) -> Vec<String> {
    match serde_json::from_str::<Command>(line) {
        Ok(cmd) => widget
            .apply(cmd)
            .into_iter()
            .filter_map(|e| serde_json::to_string(&e).ok())
            .collect(),
        Err(e) => {
            let ev = Event::Error {
                code: "bad_json".to_owned(),
                detail: e.to_string(),
            };
            serde_json::to_string(&ev).into_iter().collect()
        }
    }
}

/// Run the widget as a JSON-lines filter: read commands, write events.
pub fn run<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> std::io::Result<()> {
    let mut widget = Widget::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        for event in apply_line(&mut widget, trimmed) {
            writeln!(writer, "{event}")?;
        }
        writer.flush()?;
    }
    Ok(())
}
