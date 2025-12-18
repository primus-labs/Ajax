use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use network::p2p::{NodeConfig, P2pNet};
use serial_test::serial;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Barrier;
use tracing::{info, warn};

#[tokio::test]
#[serial]
async fn send_and_receive() {
    const BASE_PORT: usize = 5100;
    const NUM_PARTIES: usize = 35;

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

#[tokio::test]
#[serial]
async fn raw_broadcast() {
    const BASE_PORT: usize = 5200;
    const NUM_PARTIES: usize = 20;

    const ID_BROADCAST_PARTY: usize = 1;

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
                let mut node = P2pNet::new(
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

                let mut n_received_msg = 0;

                let message = format!("Hello from node {}", ID_BROADCAST_PARTY);
                if id == ID_BROADCAST_PARTY {
                    node.raw_broadcast(message.as_bytes())
                        .await
                        .expect("The message should be broadcasted");
                    node.flush_all()
                        .await
                        .expect("All the messages should be flushed");
                } else {
                    let mut buffer = [0; 128];
                    node.recv(ID_BROADCAST_PARTY, &mut buffer).await.expect(
                        "The message should be received as it was sent by the previous send",
                    );
                    assert_eq!(buffer[..message.len()], message.as_bytes()[..message.len()]);
                    n_received_msg += 1;
                }

                sender_channel
                    .send((id, n_received_msg))
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
    while let Some((id, num_received_msgs)) = results_rx.recv().await {
        if id == ID_BROADCAST_PARTY {
            assert_eq!(num_received_msgs, 0);
        } else {
            assert_eq!(num_received_msgs, 1);
        }
    }
    info!("All the parties received the same number of messages");
}

#[tokio::test]
#[serial]
async fn gossipsub_broadcast() {
    const BASE_PORT: usize = 5300;
    const NUM_PARTIES: usize = 20;

    const ID_BROADCAST_PARTY: usize = 1;

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
                let (tx, mut rx_broadcasts) = tokio::sync::mpsc::channel(100);
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

                let mut n_received_msg = 0;

                let message = format!("Hello from node {}", ID_BROADCAST_PARTY);
                if id == ID_BROADCAST_PARTY {
                    node.broadcast(message.as_bytes())
                        .await
                        .expect("The message should be broadcasted");
                } else {
                    if let Some(msg) = rx_broadcasts.recv().await {
                        // Verify that the received message is the same as the sender one.
                        assert_eq!(message.as_bytes(), msg.data);
                        n_received_msg += 1;
                    }
                }

                sender_channel
                    .send((id, n_received_msg))
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
    while let Some((id, num_received_msgs)) = results_rx.recv().await {
        if id == ID_BROADCAST_PARTY {
            assert_eq!(num_received_msgs, 0);
        } else {
            assert_eq!(num_received_msgs, 1);
        }
    }
    info!("All the parties received the same number of messages");
}

#[tokio::test]
#[serial]
async fn node_dropped_intentionally() {
    const BASE_PORT: usize = 5400;
    const NUM_PARTIES: usize = 20;
    const ID_DROPPING_PARTY: usize = 1;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .init();

    // Generates the key pairs for each party.
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    // Create threads for each party to simulate network communication.
    let mut handles = Vec::new();
    let (results_sx, mut results_rx) = tokio::sync::mpsc::channel(NUM_PARTIES + 2);

    let start_barrier = Arc::new(Barrier::new(NUM_PARTIES - 1));

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

                // One node drops intentionally.
                if id == ID_DROPPING_PARTY {
                    warn!(local_id = id, "Dropping party.");
                    return;
                }

                let message = format!("Hello from node {}", node.id());
                for other_id in 0..NUM_PARTIES {
                    if other_id != id && other_id != ID_DROPPING_PARTY {
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
                    if other_id != id && other_id != ID_DROPPING_PARTY {
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
        assert_eq!(num_received_msgs, NUM_PARTIES - 2);
    }
    info!("All the parties received the same number of messages");
}
