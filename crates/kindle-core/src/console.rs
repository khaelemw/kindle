//! Live console: a line-based TCP server on 127.0.0.1. Each line is forwarded to the game
//! thread, executed there, and the reply is written back followed by a line containing
//! only `\u{4}` (end of reply). Used by `kindle console` / `kindle run`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

/// Stands for a newline inside a one-line request (multi-line code from the CLI).
pub const NEWLINE: char = '\u{1}';
/// The reply to code that isn't finished yet (the client sends it again with more lines).
pub const INCOMPLETE: &str = "\u{5}";

pub const END_OF_REPLY: &str = "\u{4}";
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// A line typed into the console (or the overlay's), and where its reply goes.
pub struct ConsoleRequest {
    pub line: String,
    pub reply: Sender<String>,
}

pub fn start(port: u16, to_game: Sender<ConsoleRequest>) {
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            crate::warn!("console: cannot listen on 127.0.0.1:{port}: {e}");
            return;
        }
    };
    crate::info!("console listening on 127.0.0.1:{port}");
    std::thread::Builder::new()
        .name("kindle-console".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = to_game.clone();
                let _ = std::thread::Builder::new().name("kindle-console-client".into()).spawn(move || serve(stream, tx));
            }
        })
        .ok();
}

fn serve(stream: TcpStream, to_game: Sender<ConsoleRequest>) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    crate::debug!("console client connected {peer}");
    let Ok(mut writer) = stream.try_clone() else { return };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let (reply_tx, reply_rx) = mpsc::channel();
        if to_game.send(ConsoleRequest { line, reply: reply_tx }).is_err() {
            break;
        }
        let reply = reply_rx.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|_| "error: no reply from the game (is it paused, minimised or still loading?)".into());
        if writeln!(writer, "{reply}\n{END_OF_REPLY}").is_err() {
            break;
        }
    }
    crate::debug!("console client disconnected {peer}");
}
