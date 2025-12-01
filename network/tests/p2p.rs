use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use network::p2p::{Error, NodeConfig, P2pNet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;
use tracing::{debug, info, warn, Level};

#[tokio::test]
async fn initial_connection() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(Level::INFO)
        .init();

    const BASE_PORT: usize = 5000;
    const NUM_PARTIES: usize = 15;

    // Generates the key pairs for each party.
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    // Create threads for each party to simulate network communication.
    let mut handles = Vec::new();
    let (results_sx, mut results_rx) = tokio::sync::mpsc::channel(NUM_PARTIES + 2);

    let start_barrier = Arc::new(Barrier::new(NUM_PARTIES));

    for id in 0..NUM_PARTIES {
        handles.push(tokio::spawn({
            let key_pairs = key_pairs.clone();
            let sender_channel = results_sx.clone();
            let barrier = start_barrier.clone();
            async move {
                info!("Starting node {id}");
                // Configure the node.
                let (tx, _rx) = tokio::sync::mpsc::channel(100);
                let listen_addr =
                    Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
                let listen_addrs = vec![listen_addr];

                // Generate the node configuration.
                let node_config = NodeConfig::new(listen_addrs, key_pairs[id].clone());

                // Create the node and listen to the provided addresses.
                let node = P2pNet::new(id, node_config, tx)
                    .await
                    .expect("The node must be created correctly");
                node.listen()
                    .await
                    .expect("The node should listen to incoming connections");

                // Wait for all the nodes to be listening.
                barrier.wait().await;

                // Dial to the other parties. This block creates a vector of parties to dial
                // in the following way:
                //   [ party_0: (encoded_id_0, usize_id_0, addresses_to_dial_to_party_0) ]
                //   [ party_1: (encoded_id_1, usize_id_1, addresses_to_dial_to_party_1) ]
                //   ...
                //   [ party_n: (encoded_id_n, usize_id_n, addresses_to_dial_to_party_n) ]
                //
                // Here, the encoded_id means the ID in the libp2p jargon, which is basically a
                // hash of the public key.
                let mut remote_peers = Vec::new();
                for other_id in 0..NUM_PARTIES {
                    if id != other_id {
                        let dial_addr = Multiaddr::from_str(&format!(
                            "/ip4/127.0.0.1/tcp/{}",
                            BASE_PORT + other_id
                        ))
                        .unwrap();
                        remote_peers.push((
                            PeerId::from_public_key(&key_pairs[other_id].public()),
                            other_id,
                            vec![dial_addr],
                        ));
                    }
                }

                debug!("List of remote peers for peer {id}: {remote_peers:?}");
                node.dial(remote_peers.clone())
                    .await
                    .expect("The node should dial other peers correctly");

                // Wait a bit that the connections are completely done.
                barrier.wait().await;
                tokio::time::sleep(Duration::from_millis(1000)).await;

                // Now that we are connected using the dial, we open streams between the parties.
                let peer_ids: Vec<PeerId> = remote_peers
                    .into_iter()
                    .map(|(peer_id, _, _)| peer_id)
                    .collect();
                for peer_id in peer_ids {
                    if node.encoded_peer_id() < peer_id {
                        node.open_stream(peer_id)
                            .await
                            .expect("The peer ID should be connected using a raw stream");
                    }
                }

                info!(local_id = id, "Waiting for all the nodes to open streams");
                barrier.wait().await;

                let message = format!("Hello from node {}", node.id());
                for other_id in 0..NUM_PARTIES {
                    if other_id != id {
                        loop {
                            let send_result = node.send(other_id, message.as_bytes()).await;
                            match send_result {
                                Ok(bytes) => {
                                    info!("Message sent successfully from {:?} to {:?} with {:?} bytes", id, other_id, bytes);
                                    node.flush(other_id).await.expect("The message should be flushed");
                                    break;
                                }
                                Err(Error::StreamNotInDatabase(other_party_id)) => {
                                    warn!("Error sending message from {:?} to {:?}. Stream not connected yet. Resending message again", id, other_party_id);
                                }
                                Err(Error::PeerIdNotInDatabase(other_party_id)) => {
                                    warn!("Error sending message from {:?} to {:?}. Peer ID not in database yet. Resending message again", id, other_party_id);
                                }
                                Err(e) => {
                                    panic!("Error sending message from {:?} to {:?}: {:?}", id, other_id, e);
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(1000)).await;
                        }
                    }
                }

                let mut n_received_msg = 0;
                for other_id in 0..NUM_PARTIES {
                    if other_id != id {
                        let mut buffer = [0; 128];
                        node.recv(other_id, &mut buffer).await.expect(
                            "The message should be received as it was sent by the previous send",
                        );
                        n_received_msg += 1;
                    }
                }

                sender_channel
                    .send(n_received_msg)
                    .await
                    .expect("The process has finished so the message should be sent");

                barrier.wait().await;
                info!("Node process finished for peer {id}. Node will be destroyed.");

            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    drop(results_sx);

    // Check that each party received the same number of messages.
    info!("Checking that each party received the same number of messages");
    while let Some(num_received_msgs) = results_rx.recv().await {
        assert_eq!(num_received_msgs, NUM_PARTIES - 1);
    }
    info!("All the parties received the same number of messages");
}
