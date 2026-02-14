use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::protocol::{ClientMessage, Player};
use crate::settings::SERVER_ADDR;
use crate::sketch::ClientState;

pub fn spawn_networking_task(state: Arc<Mutex<ClientState>>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut stream = TcpStream::connect(SERVER_ADDR).await.unwrap();

            let mut id_buffer = [0u8; 4];
            stream.read_exact(&mut id_buffer).await.unwrap();
            let player_id = u32::from_be_bytes(id_buffer);
            println!("Player id: {}", player_id);

            {
                let mut locked_state = state.lock().unwrap();
                locked_state.player_id = Some(player_id);

                // make a new player
                locked_state
                    .players
                    .insert(player_id, Player::new(player_id));
            }

            loop {
                let (_, pos, vel) = {
                    let locked_state = state.lock().unwrap();
                    if let Some(player) = locked_state.players.get(&player_id) {
                        (player.id, player.pos, player.vel)
                    } else {
                        continue;
                    }
                }; // Lock is released here.

                let message = ClientMessage::PlayerUpdate {
                    id: player_id,
                    pos,
                    vel,
                };
                let json_message = serde_json::to_string(&message).unwrap();
                stream.write_all(json_message.as_bytes()).await.unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(32)).await;
            }
        });
    });
}
