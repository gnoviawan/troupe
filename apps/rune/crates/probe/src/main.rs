use client::{
    Config,
    cursor::Cursor,
    http::HttpRequest,
    link::Status,
    native::{Driver, http_send},
    ws,
};
use std::time::Duration;

/// Without `--follow`, how long `chat` waits for the next output of its replay.
const IDLE: Duration = Duration::from_secs(30);

/// A refused token or protocol stops the client for good: fail with it.
fn halted(status: Status) -> Result<(), Box<dyn std::error::Error>> {
    match status {
        Status::Halted { code, message } => Err(format!("{code}: {message}").into()),
        _ => Ok(()),
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("troupe-probe: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut url = None;
    let mut token = None;
    let command = loop {
        match args.next().as_deref() {
            Some("--url") => url = args.next(),
            Some("--token") => token = args.next(),
            Some(c) => break c.to_owned(),
            None => {
                return Err(
                    "usage: troupe-probe --url URL --token TOKEN projects|chat ID --follow".into(),
                );
            }
        }
    };
    let config = Config {
        base_url: url.ok_or("--url required")?,
        token,
        device_id: "probe".into(),
        client_instance_id: format!("probe-{}", std::process::id()),
        seed: u64::from(std::process::id()),
    };
    match command.as_str() {
        "projects" => println!(
            "{}",
            serde_json::to_string_pretty(&http_send(&config, &HttpRequest::get("/projects"))?)?
        ),
        "chat" => {
            let id = args.next().ok_or("session id required")?;
            let mut follow = false;
            for arg in args.by_ref() {
                match arg.as_str() {
                    "--follow" => follow = true,
                    _ => return Err(format!("unknown chat option {arg}").into()),
                }
            }
            let mut driver = Driver::new(ws::WsClient::new(config));
            driver.command(ws::Command::Subscribe {
                session_id: id,
                cursor: Cursor::default(),
            });
            driver.start();
            // Without --follow, chat ends once the subscribe reply came and
            // the events it counted as replayed (seq <= headSeq) were printed
            // or one at headSeq was, or once a snapshot was. `replay` is
            // (headSeq, events still due).
            let mut replay: Option<(u64, u64)> = None;
            let mut printed = 0;
            loop {
                match driver.next(IDLE) {
                    Some(ws::Output::Event(e)) => {
                        println!("{}", serde_json::to_string(&e)?);
                        if e.sid.is_some() {
                            printed = printed.max(e.seq);
                        }
                        if let Some((head, due)) = &mut replay
                            && e.sid.is_some()
                            && e.seq <= *head
                        {
                            *due = due.saturating_sub(1);
                        }
                    }
                    Some(ws::Output::Snapshot(s)) => {
                        println!("{}", serde_json::to_string(&s)?);
                        replay = Some((s.head_seq, 0));
                    }
                    Some(ws::Output::Subscribed(r)) => replay = Some((r.head_seq, r.replayed)),
                    Some(ws::Output::SessionLost { error, .. }) => {
                        return Err(error.message.into());
                    }
                    Some(ws::Output::Status(status)) => halted(status)?,
                    Some(_) => {}
                    None if follow => {}
                    None => return Err("timed out before the replay finished".into()),
                }
                if !follow && replay.is_some_and(|(head, due)| due == 0 || printed >= head) {
                    break;
                }
            }
        }
        "terminal" => {
            use client::terminal::{Command, Output, TerminalClient};
            use std::io::Write;
            let id = args.next().ok_or("terminal id required")?;
            let mut claim = None;
            let mut attach = false;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--attach" => attach = true,
                    "--claim" => claim = args.next(),
                    _ => return Err("unknown terminal option".into()),
                }
            }
            if !attach {
                return Err("--attach required".into());
            }
            let mut driver = Driver::new(TerminalClient::new(config));
            driver.command(Command::Attach {
                terminal_id: id,
                claim: claim.ok_or("--claim required")?,
                log_epoch: None,
                last_seq: 0,
            });
            driver.start();
            loop {
                match driver.next(IDLE) {
                    Some(Output::Replay {
                        terminal_id,
                        gap: true,
                        bytes,
                        ..
                    }) => {
                        eprintln!("troupe-probe: terminal {terminal_id} replay lost output");
                        std::io::stdout().write_all(&bytes)?;
                        std::io::stdout().flush()?;
                    }
                    Some(Output::Replay { bytes, .. } | Output::Data { bytes, .. }) => {
                        std::io::stdout().write_all(&bytes)?;
                        std::io::stdout().flush()?;
                    }
                    Some(Output::Lost { error, .. }) => return Err(error.message.into()),
                    Some(Output::Status(status)) => halted(status)?,
                    Some(_) | None => {}
                }
            }
        }
        _ => return Err("unsupported command".into()),
    }
    Ok(())
}
