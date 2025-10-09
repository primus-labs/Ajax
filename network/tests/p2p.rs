use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use network::p2p::{NodeConfig, P2pNet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, Level};

#[tokio::test]
async fn initial_connection() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(Level::DEBUG)
        .init();

    const BASE_PORT: usize = 5000;
    const NUM_PARTIES: usize = 6;

    // Generates the key pairs for each party.
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    // Create threads for each party to simulate network communication.
    let mut handles = Vec::new();
    let (results_sx, mut results_rx) = tokio::sync::mpsc::channel(NUM_PARTIES + 2);

    for id in 0..NUM_PARTIES {
        handles.push(tokio::spawn({
            let key_pairs = key_pairs.clone();
            let sender_channel = results_sx.clone();
            async move {
                info!("Starting node {id}");
                // Configure the node.
                let (tx, _rx) = tokio::sync::mpsc::channel(100);
                let listen_addr =
                    Multiaddr::from_str(&format!("/ip4/0.0.0.0/udp/{}/quic-v1", BASE_PORT + id))
                        .unwrap();
                let listen_addrs = vec![listen_addr];

                // Generate the node configuration.
                let node_config = NodeConfig::new(listen_addrs, key_pairs[id].clone());

                // Create the node and listen on the provided addresses.
                let node = Arc::new(P2pNet::new(id, node_config, tx).unwrap());
                node.listen()
                    .await
                    .expect("The node should listen to incoming connections");

                tokio::spawn({
                    let node = node.clone();
                    async move {
                        node.run().await.unwrap();
                    }
                });

                // Dial to the other parties.
                let mut remote_peers = Vec::new();
                for other_id in 0..NUM_PARTIES {
                    if id != other_id {
                        let dial_addr = Multiaddr::from_str(&format!(
                            "/ip4/0.0.0.0/udp/{}/quic-v1",
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
                node.dial(remote_peers)
                    .await
                    .expect("The node should dial other peers correctly");

                let message = format!("Hello from node {}", node.id());
                for other_id in 0..NUM_PARTIES {
                    if other_id != id {
                        node.send(other_id, message.as_bytes())
                            .await
                            .expect("Greeting message should be sent");
                    }
                }

                node.flush_all()
                    .await
                    .expect("All the messages should be sent before continuing.");

                let mut n_received_msg = 0;
                for other_id in 0..NUM_PARTIES {
                    if other_id != id {
                        let mut buffer = Vec::new();
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

                info!("Node process finished for peer {id}. Node will be destroyed.");
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Check that each party received the same number of messages.
    while let Some(num_received_msgs) = results_rx.recv().await {
        assert_eq!(num_received_msgs, NUM_PARTIES - 1);
    }
}
