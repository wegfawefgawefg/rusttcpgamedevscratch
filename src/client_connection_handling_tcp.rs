use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use glam::Vec2;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SERVER_ADDR: &str = "127.0.0.1:8080";
use lazy_static::lazy_static;

use crate::common::client_to_server::ClientToServerMessage;
use crate::common::server_to_client::ServerToClientMessage;

lazy_static! {
    pub static ref INCOMING_MESSAGE_QUEUE: Arc<ArrayQueue<ServerToClientMessage>> =
        Arc::new(ArrayQueue::new(1000));
    pub static ref OUTBOUND_MESSAGE_QUEUE: Arc<ArrayQueue<ClientToServerMessage>> =
        Arc::new(ArrayQueue::new(1000));
    pub static ref SERVER_DISCONNECTED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref CLIENT_ID: Arc<AtomicU32> = Arc::new(AtomicU32::new(0));
}

// #[tokio::main]
// async fn main() -> tokio::io::Result<()> {

//     let mut state = State::new();
//     loop {
//         // lets send a chat message
//         let message = ClientToServerMessage::ChatMessage {
//             message: "Hey Man!".to_string(),
//         };
//         if OUTBOUND_MESSAGE_QUEUE.push(message).is_err() {
//             eprintln!("Outbound message queue full: dropping message");
//         }

//         process_message_queue();
//         step(&mut state);
//         tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//     }
// }

pub async fn disconnect_from_server() {}

////////////////////////    CLIENT RX/TX TASKS    ////////////////////////

pub async fn init_connection() -> tokio::io::Result<()> {
    let stream = TcpStream::connect(SERVER_ADDR).await?;
    let (mut read_half, write_half) = stream.into_split();

    // Receive the assigned ID from the server
    let mut id_buffer = [0u8; 4];
    read_half.read_exact(&mut id_buffer).await?;
    let client_id = u32::from_be_bytes(id_buffer);
    CLIENT_ID.store(client_id, Ordering::SeqCst);

    tokio::spawn(receive_incoming_messages(client_id, read_half));
    tokio::spawn(transmit_outbound_messages(write_half));
    Ok(())
}

pub async fn receive_incoming_messages(
    id: u32,
    mut socket_read_half: tokio::net::tcp::OwnedReadHalf,
) -> io::Result<()> {
    let mut buffer = [0; 1024];
    loop {
        let nbytes = socket_read_half.read(&mut buffer).await?;
        if nbytes == 0 {
            SERVER_DISCONNECTED.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let result: Result<ServerToClientMessage, _> = bincode::deserialize(&buffer[..nbytes]);
        match result {
            Ok(message) => {
                if INCOMING_MESSAGE_QUEUE.push(message).is_err() {
                    eprintln!("Inbound message queue full: dropping message from {}", id);
                }
            }
            Err(e) => {
                eprintln!("Error parsing client data: {:?}", e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

pub async fn transmit_outbound_messages(
    mut socket_write_half: tokio::net::tcp::OwnedWriteHalf,
) -> io::Result<()> {
    loop {
        // check for disconnect message from rx task
        if SERVER_DISCONNECTED.load(Ordering::SeqCst) {
            disconnect_from_server().await; // TODO: implement cleanup
            return Ok(());
        }

        // transmit any outbound messages
        if let Some(message) = OUTBOUND_MESSAGE_QUEUE.pop() {
            match bincode::serialize(&message) {
                Ok(binary_message) => {
                    socket_write_half.write_all(&binary_message).await?;
                }
                Err(e) => {
                    eprintln!("Error serializing message: {:?}", e);
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
