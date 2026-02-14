use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use serde::{Deserialize, Serialize};

const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:8080";

#[derive(Clone, Copy)]
struct PlayerPos {
    x: f32,
    y: f32,
}

#[derive(Default)]
struct SharedState {
    clients: HashMap<u32, mpsc::Sender<String>>,
    positions: HashMap<u32, PlayerPos>,
}

type Shared = Arc<Mutex<SharedState>>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    Position { x: f32, y: f32 },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Welcome { id: u32 },
    Position { id: u32, x: f32, y: f32 },
    PlayerLeft { id: u32 },
}

fn main() -> std::io::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_SERVER_ADDR.to_string());
    let listener = TcpListener::bind(&addr)?;
    let next_client_id = Arc::new(AtomicU32::new(1));
    let shared: Shared = Arc::new(Mutex::new(SharedState::default()));

    println!("server listening on {addr}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let client_id = next_client_id.fetch_add(1, Ordering::Relaxed);
                let shared = Arc::clone(&shared);
                thread::spawn(move || {
                    if let Err(err) = handle_client(client_id, stream, shared) {
                        eprintln!("client {client_id} error: {err}");
                    }
                });
            }
            Err(err) => {
                eprintln!("accept failed: {err}");
            }
        }
    }

    Ok(())
}

fn handle_client(client_id: u32, stream: TcpStream, shared: Shared) -> std::io::Result<()> {
    let read_stream = stream.try_clone()?;
    let mut write_stream = stream;
    let (tx, rx) = mpsc::channel::<String>();

    {
        let mut state = shared.lock().expect("shared mutex poisoned");
        state.clients.insert(client_id, tx);
    }

    send_direct(
        &mut write_stream,
        &ServerMessage::Welcome { id: client_id },
    );
    send_existing_positions(&mut write_stream, client_id, &shared)?;

    let writer = thread::spawn(move || -> std::io::Result<()> {
        while let Ok(message) = rx.recv() {
            writeln!(write_stream, "{message}")?;
            write_stream.flush()?;
        }
        Ok(())
    });

    let mut reader = BufReader::new(read_stream);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }

        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        let incoming: ClientMessage = match serde_json::from_str(trimmed) {
            Ok(msg) => msg,
            Err(err) => {
                eprintln!("bad message from client {client_id}: {err}");
                continue;
            }
        };

        let ClientMessage::Position { x, y } = incoming;
        {
            let mut state = shared.lock().expect("shared mutex poisoned");
            state.positions.insert(client_id, PlayerPos { x, y });
        }
        broadcast_json(
            &shared,
            Some(client_id),
            &ServerMessage::Position { id: client_id, x, y },
        );
    }

    {
        let mut state = shared.lock().expect("shared mutex poisoned");
        state.clients.remove(&client_id);
        state.positions.remove(&client_id);
    }

    broadcast_json(&shared, Some(client_id), &ServerMessage::PlayerLeft { id: client_id });
    let _ = writer.join();
    Ok(())
}

fn send_existing_positions(
    stream: &mut TcpStream,
    new_client_id: u32,
    shared: &Shared,
) -> std::io::Result<()> {
    let snapshots = {
        let state = shared.lock().expect("shared mutex poisoned");
        state
            .positions
            .iter()
            .filter_map(|(&id, pos)| {
                if id == new_client_id {
                    None
                } else {
                    Some((id, *pos))
                }
            })
            .collect::<Vec<_>>()
    };

    for (id, pos) in snapshots {
        send_direct(
            stream,
            &ServerMessage::Position {
                id,
                x: pos.x,
                y: pos.y,
            },
        );
    }
    Ok(())
}

fn send_direct(stream: &mut TcpStream, message: &ServerMessage) {
    if let Ok(payload) = serde_json::to_string(message) {
        let _ = writeln!(stream, "{payload}");
        let _ = stream.flush();
    }
}

fn broadcast_json(shared: &Shared, exclude_id: Option<u32>, message: &ServerMessage) {
    let payload = match serde_json::to_string(message) {
        Ok(v) => v,
        Err(_) => return,
    };
    let recipients = {
        let state = shared.lock().expect("shared mutex poisoned");
        state
            .clients
            .iter()
            .filter_map(|(&id, tx)| {
                if Some(id) == exclude_id {
                    None
                } else {
                    Some(tx.clone())
                }
            })
            .collect::<Vec<_>>()
    };

    for tx in recipients {
        let _ = tx.send(payload.clone());
    }
}
