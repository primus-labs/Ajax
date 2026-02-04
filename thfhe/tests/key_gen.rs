use algebra::Field;
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use mpc::DNBackend;
use network::p2p::NodeConfig;
use rand::SeedableRng;
use std::str::FromStr;
use thfhe::{Fp, KeyGen, DEFAULT_128_BITS_PARAMETERS};
use tracing_subscriber::EnvFilter;

const NUM_PARTIES: usize = 7;
const THRESHOLD: usize = 3;
const RING_MODULUS: u64 = Fp::MODULUS_VALUE;
const TRIPLES_REQUIRED: usize = 100;

static INIT: std::sync::Once = std::sync::Once::new();

pub fn setup_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        tracing_subscriber::fmt()
            .with_target(false)
            .with_level(true)
            .with_env_filter(filter)
            .with_file(true)
            .with_line_number(true)
            .with_test_writer()
            .init();
    });
}

#[ignore]
#[tokio::test]
async fn key_generation_correctness() {
    setup_tracing();
    const BASE_PORT: usize = 5400;

    // Generates the key pairs for each party to establish the secure connections between them.
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    let mut handles = Vec::new();
    for party_id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        let handler = tokio::spawn(async move {
            // Sets the information of the remote peers.
            let mut remote_peers = Vec::new();
            for other_id in 0..NUM_PARTIES {
                if party_id != other_id {
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

            // Establishes the listen addresses for the current peer.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + party_id))
                    .unwrap();

            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[party_id].clone();

            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

            let rng = &mut rand::rngs::StdRng::seed_from_u64(1);
            let parameters = &DEFAULT_128_BITS_PARAMETERS;

            // Set up the DN backend.
            let mut backend = DNBackend::<RING_MODULUS>::new(
                party_id,
                NUM_PARTIES,
                THRESHOLD,
                TRIPLES_REQUIRED,
                node_config,
                remote_peers,
                parameters.ring_dimension(),
                true,
                true,
            )
            .await
            .unwrap();
            let (_, _, _) = KeyGen::generate_mpc_key_pair(&mut backend, **parameters, rng).await;
        });
        handles.push(handler);
    }

    for handler in handles {
        handler.await.unwrap();
    }
}
