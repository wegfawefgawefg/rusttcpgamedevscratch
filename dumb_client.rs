use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use glam::Vec2;
use tokio::net::UdpSocket;

const SERVER_ADDR: &str = "127.0.0.1:8080";
use lazy_static::lazy_static;
use uuid::Uuid;

lazy_static! {
    pub static ref INCOMING_MESSAGE_QUEUE: Arc<ArrayQueue<ServerToClientMessage>> =
        Arc::new(ArrayQueue::new(1000));
    pub static ref OUTBOUND_MESSAGE_QUEUE: Arc<ArrayQueue<ClientToServerMessage>> =
        Arc::new(ArrayQueue::new(1000));
    pub static ref SERVER_DISCONNECTED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref CLIENT_UUID: Uuid = Uuid::new_v4();
}

pub struct State {
    pub player_pos: Vec2,
    pub player_vel: Vec2,
}

impl State {
    pub fn new() -> Self {
        Self {
            player_pos: Vec2::new(0.0, 0.0),
            player_vel: Vec2::new(0.0, 0.0),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let result = init_connection().await;
    if let Err(e) = result {
        eprintln!("Error connecting to server: {:?}", e);
        return Ok(());
    }
    let mut state = State::new();
    loop {
        // lets send a chat message
        let message = ClientToServerMessage::ChatMessage {
            message: "Hey Man!".to_string(),
        };
        if OUTBOUND_MESSAGE_QUEUE.push(message).is_err() {
            eprintln!("Outbound message queue full: dropping message");
        }

        process_message_queue();
        step(&mut state);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

fn step(state: &mut State) {
    state.player_pos += state.player_vel;
}

pub fn process_message_queue() {
    while let Some(message) = INCOMING_MESSAGE_QUEUE.pop() {
        match message {
            ServerToClientMessage::Welcome { server_message } => {
                println!("Server says: {}", server_message);
            }
            ServerToClientMessage::ClientJoined { id } => {
                println!("Player {} joined", id);
            }
            ServerToClientMessage::ClientLeft { id } => {
                println!("Player {} left", id);
            }
            ServerToClientMessage::ChatMessage { from, message } => {
                println!("{} says: {}", from, message);
            }
            _ => {
                eprintln!("Unknown message type");
            }
        }
    }
}

pub async fn disconnect_from_server() {}

////////////////////////    CLIENT RX/TX TASKS    ////////////////////////

pub async fn init_connection() -> tokio::io::Result<()> {
    println!("connecting");
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(SERVER_ADDR).await?;

    println!("connected");
    let a_socket = Arc::new(socket);

    println!("spawning network tasks");
    tokio::spawn(receive_incoming_messages(a_socket.clone()));
    tokio::spawn(transmit_outbound_messages(a_socket.clone()));
    Ok(())
}

pub async fn receive_incoming_messages(socket: Arc<UdpSocket>) -> io::Result<()> {
    let mut buffer = [0; 1024];
    loop {
        let nbytes = socket.recv(&mut buffer).await?;
        let result: Result<ServerToClientMessage, _> = bincode::deserialize(&buffer[..nbytes]);
        match result {
            Ok(message) => {
                if INCOMING_MESSAGE_QUEUE.push(message).is_err() {
                    eprintln!("Inbound message queue full: dropping message");
                }
            }
            Err(e) => {
                eprintln!("Error parsing client data: {:?}", e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

pub async fn transmit_outbound_messages(socket: Arc<UdpSocket>) -> io::Result<()> {
    loop {
        // check for disconnect message from rx task
        if SERVER_DISCONNECTED.load(Ordering::SeqCst) {
            disconnect_from_server().await; // TODO: implement cleanup
            return Ok(());
        }

        // transmit any outbound messages
        if let Some(message) = OUTBOUND_MESSAGE_QUEUE.pop() {
            println!("Sending message: {:?}", message);
            match bincode::serialize(&message) {
                Ok(binary_message) => {
                    socket.send(&binary_message).await?;
                }
                Err(e) => {
                    eprintln!("Error serializing message: {:?}", e);
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
