use crate::common::{
    client_to_server::{ClientToServerMessage, ClientToServerMessageBundle},
    server_to_client::ServerToClientMessage,
};
use crossbeam::queue::ArrayQueue;
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

use super::settings::SERVER_ADDR;

extern crate lazy_static;

lazy_static! {
    pub static ref INCOMING_MESSAGE_QUEUE: Arc<ArrayQueue<ClientToServerMessageBundle>> =
        Arc::new(ArrayQueue::new(1000));
    pub static ref NEXT_CONNECTION_ID: Arc<AtomicU32> = Arc::new(AtomicU32::new(0));
    pub static ref CLIENT_OUTBOUND_MAILBOXES: RwLock<HashMap<u32, ClientMessageQueue>> =
        RwLock::new(HashMap::new());
    pub static ref CLIENT_DISCONNECTED: Arc<RwLock<HashMap<u32, Arc<AtomicBool>>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

pub type ClientMessageQueue = Arc<ArrayQueue<ServerToClientMessage>>;

pub fn get_next_connection_id() -> u32 {
    NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

pub async fn init() {
    let listener = TcpListener::bind(SERVER_ADDR).await.unwrap();
    tokio::spawn(accept_connections(listener));
}

pub async fn accept_connections(listener: TcpListener) -> tokio::io::Result<()> {
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_connection(socket));
    }
}

////////////////////////    CLIENT RX/TX TASKS    ////////////////////////

pub async fn handle_connection(mut socket: TcpStream) -> tokio::io::Result<()> {
    let id = add_client().await;
    socket.write_all(&id.to_be_bytes()).await?;

    // announce that theres a new connection
    let to_self_message = ClientToServerMessageBundle {
        client_id: id,
        message: ClientToServerMessage::Connect,
    };
    if INCOMING_MESSAGE_QUEUE.push(to_self_message).is_err() {
        eprintln!(
            "Inbound message queue full: dropping disconnect message from {}",
            id
        );
    }

    let (mut socket_read_half, socket_write_half) = socket.into_split();
    tokio::spawn(continuously_transmit_any_outbound_messages(
        id,
        socket_write_half,
    ));

    let mut buffer = [0; 1024];
    loop {
        let nbytes = socket_read_half.read(&mut buffer).await?;
        if nbytes == 0 {
            // signal that the client has disconnected, via atomic bool
            let disconnect_message = ClientToServerMessageBundle {
                client_id: id,
                message: ClientToServerMessage::Disconnect,
            };
            if INCOMING_MESSAGE_QUEUE.push(disconnect_message).is_err() {
                eprintln!(
                    "Inbound message queue full: dropping disconnect message from {}",
                    id
                );
            }
            let client_disconnected_read = CLIENT_DISCONNECTED.read().await;
            if let Some(disconnected) = client_disconnected_read.get(&id) {
                disconnected.store(true, Ordering::SeqCst);
            }
            return Ok(());
        }

        let result: Result<ClientToServerMessage, _> = bincode::deserialize(&buffer[..nbytes]);
        match result {
            Ok(result) => {
                let message_bundle = ClientToServerMessageBundle {
                    client_id: id,
                    message: result,
                };
                if INCOMING_MESSAGE_QUEUE.push(message_bundle).is_err() {
                    eprintln!("Inbound message queue full: dropping message from {}", id);
                }
            }
            Err(e) => {
                eprintln!("Error parsing client data: {:?}", e);
            }
        }
    }
}

pub async fn continuously_transmit_any_outbound_messages(
    id: u32,
    mut socket_write_half: tokio::net::tcp::OwnedWriteHalf,
) -> io::Result<()> {
    loop {
        // check for disconnect
        {
            let client_status_read = CLIENT_DISCONNECTED.read().await;
            if let Some(disconnected_flag) = client_status_read.get(&id) {
                if disconnected_flag.load(Ordering::Relaxed) {
                    drop(client_status_read);
                    remove_client(id).await; //  remove client allocated bookkeeping resources
                    return Ok(());
                }
            }
            drop(client_status_read);
        }

        // transmit any outbound messages
        let clients_read = CLIENT_OUTBOUND_MAILBOXES.read().await;
        if let Some(outgoing_messages) = clients_read.get(&id) {
            if let Some(message) = outgoing_messages.pop() {
                match bincode::serialize(&message) {
                    Ok(binary_message) => {
                        socket_write_half.write_all(&binary_message).await?;
                    }
                    Err(e) => {
                        eprintln!("Error serializing message: {:?}", e);
                    }
                }
            }
        }
        drop(clients_read);

        // Some delay, or await on an event to prevent busy-waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

////////////////////////    CLIENT BOOKKEEPING    ////////////////////////
pub async fn add_client() -> u32 {
    let id = get_next_connection_id();
    let mailbox = Arc::new(ArrayQueue::new(100));

    // Insert into CLIENT_OUTBOUND_MAILBOXES
    let mut clients_write = CLIENT_OUTBOUND_MAILBOXES.write().await;
    clients_write.insert(id, mailbox);

    // Insert into CLIENT_DISCONNECTED flag map
    let disconnected = Arc::new(AtomicBool::new(false));
    let mut client_status_write = CLIENT_DISCONNECTED.write().await;
    client_status_write.insert(id, disconnected.clone());

    println!("New Connected Client: Assigned ID: {}", id);
    id
}

///  Removes client allocated bookkeeping resources.
pub async fn remove_client(id: u32) {
    // Remove from CLIENT_OUTBOUND_MAILBOXES
    let mut clients_write = CLIENT_OUTBOUND_MAILBOXES.write().await;
    clients_write.remove(&id);

    // Remove from CLIENT_DISCONNECTED flag map
    let mut client_status_write = CLIENT_DISCONNECTED.write().await;
    client_status_write.remove(&id);

    println!("Client {} network resources cleaned up.", id);
}

////////////////////////    ENQUEUE OUTBOUND MESSAGES    ////////////////////////
pub async fn send_to_one_client(client_id: u32, message: ServerToClientMessage) {
    let clients_read = CLIENT_OUTBOUND_MAILBOXES.read().await;
    if let Some(queue) = clients_read.get(&client_id) {
        if queue.push(message).is_err() {
            eprintln!("Failed to enqueue message for client {}", client_id);
        }
    } else {
        eprintln!("Failed to find client {}", client_id);
    }
}

pub async fn broadcast_to_all_except(sender_id: u32, message: ServerToClientMessage) {
    let clients_read = CLIENT_OUTBOUND_MAILBOXES.read().await;
    for (&client_id, queue) in clients_read.iter() {
        if client_id == sender_id {
            continue; // Skip the sender
        }
        if queue.push(message.clone()).is_err() {
            eprintln!("Failed to enqueue message for client {}", client_id);
        }
    }
}

pub async fn broadcast_to_all(message: ServerToClientMessage) {
    let clients_read = CLIENT_OUTBOUND_MAILBOXES.read().await;
    for (_, queue) in clients_read.iter() {
        if queue.push(message.clone()).is_err() {
            eprintln!("Failed to enqueue message for client");
        }
    }
}
