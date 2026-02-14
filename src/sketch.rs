use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use raylib::prelude::*;
use serde::{Deserialize, Serialize};

pub const FRAMES_PER_SECOND: u32 = 60;
const PLAYER_SPEED: f32 = 260.0;
const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:8080";

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClientMessage {
    Position { x: f32, y: f32 },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Welcome { id: u32 },
    Position { id: u32, x: f32, y: f32 },
    PlayerLeft { id: u32 },
}

struct NetClient {
    incoming: Receiver<ServerMessage>,
    outgoing: SyncSender<ClientMessage>,
}

pub struct ClientState {
    pub running: bool,
    pub player_id: Option<u32>,
    pub player_pos: Vector2,
    pub remote_players: HashMap<u32, Vector2>,
    pub player_radius: f32,
    net: Option<NetClient>,
}

impl ClientState {
    fn new(screen_w: i32, screen_h: i32, net: Option<NetClient>) -> Self {
        Self {
            running: true,
            player_id: None,
            player_pos: Vector2::new((screen_w / 2) as f32, (screen_h / 2) as f32),
            remote_players: HashMap::new(),
            player_radius: 16.0,
            net,
        }
    }
}

pub fn run(server_addr: Option<String>) {
    let addr = server_addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.to_string());
    let net = connect_network(&addr);

    let screen_w = 960;
    let screen_h = 540;
    let (mut rl, thread) = raylib::init()
        .size(screen_w, screen_h)
        .title("rusttcpgamedevscratch - client sketch")
        .resizable()
        .build();
    rl.set_target_fps(FRAMES_PER_SECOND);

    let mut state = ClientState::new(screen_w, screen_h, net);
    while state.running && !rl.window_should_close() {
        let dt = rl.get_frame_time();
        step(&mut rl, &mut state, dt);
        draw(&mut rl, &thread, &state);
    }
}

pub fn step(rl: &mut RaylibHandle, state: &mut ClientState, dt: f32) {
    process_network_messages(state);

    let mut axis = Vector2::new(0.0, 0.0);
    if rl.is_key_down(KeyboardKey::KEY_W) || rl.is_key_down(KeyboardKey::KEY_UP) {
        axis.y -= 1.0;
    }
    if rl.is_key_down(KeyboardKey::KEY_S) || rl.is_key_down(KeyboardKey::KEY_DOWN) {
        axis.y += 1.0;
    }
    if rl.is_key_down(KeyboardKey::KEY_A) || rl.is_key_down(KeyboardKey::KEY_LEFT) {
        axis.x -= 1.0;
    }
    if rl.is_key_down(KeyboardKey::KEY_D) || rl.is_key_down(KeyboardKey::KEY_RIGHT) {
        axis.x += 1.0;
    }

    if axis.x != 0.0 || axis.y != 0.0 {
        let inv_len = 1.0 / (axis.x * axis.x + axis.y * axis.y).sqrt();
        axis.x *= inv_len;
        axis.y *= inv_len;
    }

    state.player_pos.x += axis.x * PLAYER_SPEED * dt;
    state.player_pos.y += axis.y * PLAYER_SPEED * dt;

    let sw = rl.get_screen_width() as f32;
    let sh = rl.get_screen_height() as f32;
    state.player_pos.x = state
        .player_pos
        .x
        .clamp(state.player_radius, sw - state.player_radius);
    state.player_pos.y = state
        .player_pos
        .y
        .clamp(state.player_radius, sh - state.player_radius);

    if let Some(net) = &state.net {
        let _ = net.outgoing.try_send(ClientMessage::Position {
            x: state.player_pos.x,
            y: state.player_pos.y,
        });
    }
}

fn draw(rl: &mut RaylibHandle, thread: &RaylibThread, state: &ClientState) {
    let mut d = rl.begin_drawing(thread);
    d.clear_background(Color::new(20, 22, 28, 255));

    d.draw_text("Raylib client sketch", 20, 20, 28, Color::RAYWHITE);
    d.draw_text("Move: WASD / Arrow keys", 20, 58, 18, Color::LIGHTGRAY);
    d.draw_text("Blue = you, green = remote players", 20, 82, 18, Color::LIGHTGRAY);

    if let Some(id) = state.player_id {
        d.draw_text(&format!("you: {id}"), 20, 106, 18, Color::YELLOW);
    } else {
        d.draw_text("connecting...", 20, 106, 18, Color::ORANGE);
    }

    for (&id, pos) in &state.remote_players {
        d.draw_circle_v(*pos, state.player_radius, Color::LIME);
        d.draw_circle_lines(
            pos.x as i32,
            pos.y as i32,
            state.player_radius,
            Color::DARKGREEN,
        );
        d.draw_text(
            &format!("{id}"),
            pos.x as i32 - 5,
            pos.y as i32 - 28,
            16,
            Color::RAYWHITE,
        );
    }

    d.draw_circle_v(state.player_pos, state.player_radius, Color::SKYBLUE);
    d.draw_circle_lines(
        state.player_pos.x as i32,
        state.player_pos.y as i32,
        state.player_radius,
        Color::WHITE,
    );
}

fn connect_network(addr: &str) -> Option<NetClient> {
    let stream = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("network disabled (failed to connect to {addr}): {err}");
            return None;
        }
    };
    let read_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(err) => {
            eprintln!("network disabled (failed to clone stream): {err}");
            return None;
        }
    };

    let (incoming_tx, incoming_rx) = mpsc::channel::<ServerMessage>();
    let (outgoing_tx, outgoing_rx) = mpsc::sync_channel::<ClientMessage>(16);

    thread::spawn(move || {
        let mut reader = BufReader::new(read_stream);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = match reader.read_line(&mut line) {
                Ok(v) => v,
                Err(_) => break,
            };
            if bytes == 0 {
                break;
            }
            if let Ok(msg) = serde_json::from_str::<ServerMessage>(line.trim_end()) {
                let _ = incoming_tx.send(msg);
            }
        }
    });

    thread::spawn(move || {
        let mut socket = stream;
        while let Ok(message) = outgoing_rx.recv() {
            if let Ok(payload) = serde_json::to_string(&message) {
                if writeln!(socket, "{payload}").is_err() {
                    break;
                }
                if socket.flush().is_err() {
                    break;
                }
            }
        }
    });

    Some(NetClient {
        incoming: incoming_rx,
        outgoing: outgoing_tx,
    })
}

fn process_network_messages(state: &mut ClientState) {
    let Some(net) = &state.net else {
        return;
    };

    while let Ok(message) = net.incoming.try_recv() {
        match message {
            ServerMessage::Welcome { id } => {
                state.player_id = Some(id);
            }
            ServerMessage::Position { id, x, y } => {
                if state.player_id == Some(id) {
                    continue;
                }
                state.remote_players.insert(id, Vector2::new(x, y));
            }
            ServerMessage::PlayerLeft { id } => {
                state.remote_players.remove(&id);
            }
        }
    }
}
