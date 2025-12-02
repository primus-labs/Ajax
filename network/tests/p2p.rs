use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use network::p2p::{NodeConfig, P2pNet};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Barrier;
use tracing::{info, Level};

#[tokio::test]
async fn initial_connection() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(Level::ERROR)
        .init();

    const BASE_PORT: usize = 5000;
    const NUM_PARTIES: usize = 20;

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

                // This block creates a vector of parties to dial
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

                // Generate the node configuration.
                let node_config = NodeConfig::new(listen_addrs, key_pairs[id].clone());

                // Create the node and listen to the provided addresses.
                let notifier_network_ready = Arc::new(tokio::sync::Notify::new());
                let node = P2pNet::new(
                    id,
                    node_config,
                    remote_peers,
                    tx,
                    notifier_network_ready.clone(),
                    NUM_PARTIES,
                )
                .await
                .expect("The node must be created correctly");

                info!(local_id = id, "Waiting for all the nodes to open streams");
                notifier_network_ready.notified().await;

                let message = format!("Hello from node {}", node.id());
                for other_id in 0..NUM_PARTIES {
                    if other_id != id {
                        let send_result = node.send(other_id, message.as_bytes()).await;
                        match send_result {
                            Ok(bytes) => {
                                info!(
                                    "Message sent successfully from {:?} to {:?} with {:?} bytes",
                                    id, other_id, bytes
                                );
                                node.flush(other_id)
                                    .await
                                    .expect("The message should be flushed");
                            }
                            Err(e) => {
                                panic!(
                                    "Error sending message from {:?} to {:?}: {:?}",
                                    id, other_id, e
                                );
                            }
                        }
                    }
                }

                node.flush_all()
                    .await
                    .expect("All the messages should be flushed");

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
