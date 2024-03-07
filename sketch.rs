use std::collections::HashMap;

use glam::Vec2;
use raylib::prelude::*;

use crate::protocol::Player;

pub const FRAMES_PER_SECOND: u32 = 60;

impl ClientState {
    pub fn new() -> Self {
        Self {
            running: true,
            time_since_last_update: 0.0,

            player_id: None,

            players: HashMap::new(),
        }
    }
}

const PLAYER_SPEED: f32 = 1.0;

pub fn step(rl: &mut RaylibHandle, rlt: &mut RaylibThread, state: &mut ClientState) {
    // set the mouse
}

pub async fn process_message_queue() {
    while let Some(message_bundle) = INCOMING_MESSAGE_QUEUE.pop() {
        let client_id = message_bundle.client_id;
        match message_bundle.message {
            ClientToServerMessage::Connect => {
                println!("Client {} connected", client_id);

                // send welcome
                let outbound_message = ServerToClientMessage::Welcome {
                    server_message: "welcome to the server".to_string(),
                };
                send_to_one_client(client_id, outbound_message).await;

                // announce the join
                let outbound_message = ServerToClientMessage::PlayerJoined { id: client_id };
                broadcast_to_all_except(client_id, outbound_message).await;
            }
            ClientToServerMessage::Disconnect => {
                println!("Client {} disconnected", client_id);

                // announce the leave
                let outbound_message = ServerToClientMessage::PlayerLeft { id: client_id };
                broadcast_to_all_except(client_id, outbound_message).await;
            }
            ClientToServerMessage::ChatMessage { message } => {
                println!("{} says: {}", client_id, message);

                // broadcast the message
                let outbound_message = ServerToClientMessage::ChatMessage {
                    from: client_id,
                    message,
                };
                broadcast_to_all_except(client_id, outbound_message).await;
            }
        }
    }
}
