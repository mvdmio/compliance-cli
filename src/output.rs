use std::io::{self, IsTerminal, Write};

use serde_json::Value;

// Write errors are ignored throughout: a closed stdout or stderr leaves nowhere to report them.

pub fn print_json(value: &Value) {
    let stdout = io::stdout();
    let pretty = stdout.is_terminal();
    let _ = writeln!(stdout.lock(), "{}", render(value, pretty));
}

pub fn print_stderr_json(value: &Value) {
    let stderr = io::stderr();
    let pretty = stderr.is_terminal();
    let _ = writeln!(stderr.lock(), "{}", render(value, pretty));
}

pub fn print_stderr_raw(bytes: &[u8]) {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(bytes);
    if !bytes.ends_with(b"\n") {
        let _ = stderr.write_all(b"\n");
    }
}

fn render(value: &Value, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .expect("a serde_json::Value always serializes")
}
