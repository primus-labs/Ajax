use algebra::{Field, U64FieldEval};
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use mpc::{DNBackend, MPCBackend};
use network::p2p::NodeConfig;
use rand::{random, Rng};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use thfhe::sqrt_mod_p;
use tokio::sync::Mutex;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

// Prime field modulus for tests.
const PRIME: u64 = 9007199254614017;

static INIT: std::sync::Once = std::sync::Once::new();

pub fn setup_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .init();
    });
}

/// Tests secret sharing and reconstruction between parties.
/// Verifies that shares can be properly distributed and recombined.
#[tokio::test]
async fn test_secret_sharing_and_recovery() {
    setup_tracing();

    const NUM_PARTIES: usize = 5;
    const THRESHOLD: usize = 2;
    const BASE_PORT: usize = 50000;

    let mut rng = rand::thread_rng();

    let secrets: Vec<u64> = (0..NUM_PARTIES).map(|_| rng.gen_range(0..PRIME)).collect();

    // Create threads for each party to simulate network communication.
    let mut handles = Vec::new();

    // Generates the key pairs for each party.
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    let barrier = Arc::new(tokio::sync::Barrier::new(NUM_PARTIES));

    for id in 0..NUM_PARTIES {
        let secrets = secrets.clone();
        let key_pairs = key_pairs.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let mut dn = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            info!(local_id = id, "DN backend set up successfully");

            // Test input and reveal_to_all for each secret.
            for secret in secrets {
                // Each party takes turns being the dealer.
                for dealer_id in 0..NUM_PARTIES {
                    // Only the dealer provides the input value.
                    let input_value = if id == dealer_id { Some(secret) } else { None };
                    let share = dn.input(input_value, dealer_id).await.unwrap();
                    info!(local_id = id, "Dealer ID: {dealer_id}, got share {share}");

                    // All parties reveal and verify.
                    let result = dn.reveal_to_all(share).await.unwrap();
                    info!(local_id = id, "Reveal finished");
                    assert_eq!(result, secret, "Party {id} got incorrect result");
                }
            }

            info!(local_id = id, "Waiting for other parties to finish");
            barrier.wait().await;

            info!(local_id = id, "Test finished");

            // Return success if all tests passed for this party.
            true
        }));
    }

    // Verify all threads succeeded.
    for handle in handles {
        handle.await.unwrap();
    }
}

/// Tests the correctness of Beaver triples generation and usage.
/// Verifies that triples satisfy the relation c = a*b and can be used in multiplications.
#[tokio::test]
async fn test_triple_correctness() {
    const NUM_PARTIES: usize = 7;
    const THRESHOLD: usize = 3;
    const BASE_PORT: usize = 51400;
    const NUM_TRIPLES: usize = 100;

    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();
    let mut handles = Vec::new();

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        handles.push(tokio::spawn(async move {
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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
            // Set up the DN backend.
            let mut dn = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            for _ in 0..NUM_TRIPLES / 2 {
                // Get a triple from the buffer.
                let (share_a, share_b, share_c) = dn.next_triple().await;

                // Reveal all values.
                let revealed_a = dn.reveal_to_all(share_a).await.unwrap();
                let revealed_b = dn.reveal_to_all(share_b).await.unwrap();
                let revealed_c = dn.reveal_to_all(share_c).await.unwrap();

                let calculated_c = dn.mul(share_a, share_b).await.unwrap();

                // Verify that the revealed c matches a*b.
                let expected = U64FieldEval::<PRIME>::mul(revealed_a, revealed_b);
                let revealed_calculated_c = dn.reveal_to_all(calculated_c).await.unwrap();
                assert_eq!(
                    revealed_c, expected,
                    "Revealed triple is incorrect: c ≠ a*b"
                );

                // Verify that our calculated c matches the original c.
                assert_eq!(
                    revealed_calculated_c, revealed_c,
                    "Calculated c doesn't match original c"
                );
            }

            true
        }));
    }

    // Verify all threads succeeded.
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

/// Tests basic MPC operations including addition, multiplication, and other core functions.
/// Verifies correctness of operations with different input values.
#[tokio::test]
async fn test_mpc_operations() {
    const NUM_PARTIES: usize = 7;
    const THRESHOLD: usize = 3;
    const BASE_PORT: usize = 50200;

    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();
    let mut handles = Vec::new();

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let dn = Arc::new(Mutex::new(
                DNBackend::<PRIME>::new(
                    id,
                    NUM_PARTIES,
                    THRESHOLD,
                    10,
                    node_config,
                    remote_peers,
                    1024,
                    true,
                    true,
                )
                .await
                .unwrap(),
            ));

            // Test 1: Addition.
            let a_value = 42;
            let b_value = 99;

            // Each party gets shares.
            let share_a = if id == 0 {
                dn.lock().await.input(Some(a_value), 0).await.unwrap()
            } else {
                dn.lock().await.input(None, 0).await.unwrap()
            };

            let share_b = if id == 1 {
                dn.lock().await.input(Some(b_value), 1).await.unwrap()
            } else {
                dn.lock().await.input(None, 1).await.unwrap()
            };

            // Addition (local operation).
            let share_sum = dn.lock().await.add(share_a, share_b);
            let sum_result = dn.lock().await.reveal_to_all(share_sum).await.unwrap();
            assert_eq!(
                sum_result,
                U64FieldEval::<PRIME>::add(a_value, b_value),
                "Addition failed"
            );

            // Test 2: Multiplication (requires communication).
            let share_prod = dn.lock().await.mul(share_a, share_b).await.unwrap();
            let prod_result = dn.lock().await.reveal_to_all(share_prod).await.unwrap();
            assert_eq!(
                prod_result,
                U64FieldEval::<PRIME>::mul(a_value, b_value),
                "Multiplication failed"
            );

            // Test 3: Batch multiplication.
            let shares_a = vec![share_a, share_a, share_a];
            let shares_b = vec![share_b, share_b, share_b];

            let shares_prod = dn
                .lock()
                .await
                .mul_element_wise(&shares_a, &shares_b)
                .await
                .unwrap();
            assert_eq!(shares_prod.len(), 3, "Batch size mismatch");

            for share_p in shares_prod {
                let result = dn.lock().await.reveal_to_all(share_p).await.unwrap();
                assert_eq!(
                    result,
                    U64FieldEval::<PRIME>::mul(a_value, b_value),
                    "Batch multiplication failed"
                );
            }

            // Test 4: Inner product.
            let values_a = [1, 2, 3];
            let values_b = [4, 5, 6];
            let expected_dot = U64FieldEval::<PRIME>::add(
                U64FieldEval::<PRIME>::add(
                    U64FieldEval::<PRIME>::mul(1, 4),
                    U64FieldEval::<PRIME>::mul(2, 5),
                ),
                U64FieldEval::<PRIME>::mul(3, 6),
            );

            let shares_a = futures::future::join_all(values_a.iter().enumerate().map(|(i, &v)| {
                let dn_clone = Arc::clone(&dn);
                async move {
                    if id as usize == i % NUM_PARTIES as usize {
                        dn_clone.lock().await.input(Some(v), id).await.unwrap()
                    } else {
                        dn_clone
                            .lock()
                            .await
                            .input(None, i % NUM_PARTIES)
                            .await
                            .unwrap()
                    }
                }
            }))
            .await;

            let shares_b = futures::future::join_all(values_b.iter().enumerate().map(|(i, &v)| {
                let dn_clone = Arc::clone(&dn);
                async move {
                    if id as usize == i % NUM_PARTIES as usize {
                        dn_clone.lock().await.input(Some(v), id).await.unwrap()
                    } else {
                        dn_clone
                            .lock()
                            .await
                            .input(None, i % NUM_PARTIES)
                            .await
                            .unwrap()
                    }
                }
            }))
            .await;

            let dot_share = dn
                .lock()
                .await
                .inner_product(&shares_a, &shares_b)
                .await
                .unwrap();
            let dot_result = dn.lock().await.reveal_to_all(dot_share).await.unwrap();
            assert_eq!(dot_result, expected_dot, "Inner product failed");

            true
        }));
    }

    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

/// Tests additional MPC operations including negation, subtraction, and various constant operations.
/// Verifies correctness of operations not covered in the basic operations test.
#[tokio::test]
async fn test_untested_operations() {
    const NUM_PARTIES: usize = 7;
    const THRESHOLD: usize = 3;
    const BASE_PORT: usize = 50500;

    let mut handles = Vec::new();

    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();
    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);
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

            // Set up the DN backend.
            let mut dn = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            // Test values.
            let a_value = 42;
            let b_value = 7;

            // Get shares.
            let share_a = if id == 0 {
                dn.input(Some(a_value), 0).await.unwrap()
            } else {
                dn.input(None, 0).await.unwrap()
            };

            let share_b = if id == 1 {
                dn.input(Some(b_value), 1).await.unwrap()
            } else {
                dn.input(None, 1).await.unwrap()
            };

            // 1. Test neg operation.
            let neg_share = dn.neg(share_a);
            let neg_result = dn.reveal_to_all(neg_share).await.unwrap();
            assert_eq!(
                neg_result,
                U64FieldEval::<PRIME>::neg(a_value),
                "Negation failed"
            );

            // 2. Test sub operation.
            let sub_share = dn.sub(share_a, share_b);
            let sub_result = dn.reveal_to_all(sub_share).await.unwrap();
            assert_eq!(
                sub_result,
                U64FieldEval::<PRIME>::sub(a_value, b_value),
                "Subtraction failed"
            );

            // 3. Test mul_const operation.
            let const_value = 13;
            let mul_const_share = dn.mul_const(share_a, const_value);
            let mul_const_result = dn.reveal_to_all(mul_const_share).await.unwrap();
            assert_eq!(
                mul_const_result,
                U64FieldEval::<PRIME>::mul(a_value, const_value),
                "Multiplication by constant failed"
            );

            // 4. Test double operation.
            let double_share = dn.double(share_a);
            let double_result = dn.reveal_to_all(double_share).await.unwrap();
            assert_eq!(
                double_result,
                U64FieldEval::<PRIME>::add(a_value, a_value),
                "Double operation failed"
            );

            // 5. Test inner_product_const operation.
            let shares = vec![share_a, share_b, share_a];
            let constants = vec![3, 4, 5];
            let expected_inner = U64FieldEval::<PRIME>::add(
                U64FieldEval::<PRIME>::add(
                    U64FieldEval::<PRIME>::mul(a_value, 3),
                    U64FieldEval::<PRIME>::mul(b_value, 4),
                ),
                U64FieldEval::<PRIME>::mul(a_value, 5),
            );

            let inner_const_share = dn.inner_product_const(&shares, &constants);
            let inner_const_result = dn.reveal_to_all(inner_const_share).await.unwrap();
            assert_eq!(
                inner_const_result, expected_inner,
                "Inner product with constants failed"
            );

            true
        }));
    }

    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

/// Tests that rand_coin returns consistent values across all parties.
/// Verifies that the shared PRG produces identical sequences for each party.
#[tokio::test]
async fn test_rand_coin_consistency() {
    const NUM_PARTIES: usize = 4;
    const THRESHOLD: usize = 1;
    const BASE_PORT: usize = 50700;
    const NUM_COINS: usize = 10000;

    let mut handles = Vec::new();
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    // Create a channel to collect results from all parties
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        let tx = tx.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let mut dn = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            // Generate a sequence of random coins
            let mut coins = Vec::with_capacity(NUM_COINS);
            for _ in 0..NUM_COINS {
                coins.push(dn.shared_rand_coin());
            }

            // Send party ID and coin values to the main thread
            tx.send((id, coins)).await.unwrap();
            true
        }));
    }

    // Collect all results
    drop(tx); // Drop the extra sender so the receiver knows when to stop
    let mut all_results = Vec::new();
    while let Some((id, coins)) = rx.recv().await {
        all_results.push((id, coins));
    }

    // Verify all parties got the same values
    if !all_results.is_empty() {
        let reference_coins = &all_results[0].1;
        for (id, coins) in &all_results[1..] {
            assert_eq!(
                coins, reference_coins,
                "Party {id} got different random coins than party 0"
            );
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

#[ignore]
#[tokio::test]
async fn create_random_zero_elements_unlikely() {
    const NUM_PARTIES: usize = 4;
    const THRESHOLD: usize = 1;
    const BASE_PORT: usize = 50800;

    const LENGTH_RANDOM_ELEMENTS: usize = 4096;

    let mut handles = Vec::new();
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let mut dn = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            let random_shares = dn.create_random_elements(LENGTH_RANDOM_ELEMENTS).await;
            let random_elements = dn.reveal_slice_to_all(&random_shares).await.unwrap();
            assert!(
                !random_elements.is_empty(),
                "No random elements were created"
            );
            for element in random_elements {
                assert_ne!(element, 0, "the element is zero which is unlikely")
            }

            true
        }));
    }

    // Wait for all threads to complete
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

#[ignore]
#[tokio::test]
async fn square_random_zero_elements_unlikely() {
    const NUM_PARTIES: usize = 10;
    const THRESHOLD: usize = 2;
    const BASE_PORT: usize = 56000;

    const LENGTH_RANDOM_ELEMENTS: usize = 1024;

    let mut handles = Vec::new();
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let mut backend = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                5600,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            let random_elements = backend.create_random_elements(LENGTH_RANDOM_ELEMENTS).await;
            let square = backend
                .double_mul_element_wise(&random_elements, &random_elements)
                .await
                .unwrap();
            let square = backend.reveal_slice_to_all(&square).await.unwrap();
            let modulus = backend.modulus();

            let sqrt = square.iter().map(|&x| sqrt_mod_p(x, modulus));
            assert!(sqrt.len() > 0, "No random elements were created");
            for element in sqrt {
                assert_ne!(element, 0, "the element is zero which is unlikely")
            }

            true
        }));
    }

    // Wait for all threads to complete
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

#[ignore]
#[tokio::test]
async fn reveal_slice_to_all_correctness() {
    setup_tracing();

    const NUM_PARTIES: usize = 10;
    const THRESHOLD: usize = 2;
    const BASE_PORT: usize = 50900;

    const LENGTH_RANDOM_ELEMENTS: usize = 4096;

    const DEALER_IDX: usize = 0;

    let mut handles = Vec::new();
    let key_pairs = (0..NUM_PARTIES)
        .map(|_| Keypair::generate_ed25519())
        .collect::<Vec<_>>();

    let mut random_elements = [0u64; LENGTH_RANDOM_ELEMENTS];
    for i in 0..LENGTH_RANDOM_ELEMENTS {
        random_elements[i] = rand::random::<u64>() % PRIME;
    }

    let random_elements = Arc::new(random_elements);

    for id in 0..NUM_PARTIES {
        let key_pairs = key_pairs.clone();
        let random_elements = random_elements.clone();
        handles.push(tokio::spawn(async move {
            // Set up the DN backend.
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + id)).unwrap();
            let listen_addrs = vec![listen_addr];
            let key_pair = key_pairs[id].clone();
            // Generate the node configuration
            let node_config = NodeConfig::new(listen_addrs, key_pair);

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

            // Set up the DN backend.
            let mut backend = DNBackend::<PRIME>::new(
                id,
                NUM_PARTIES,
                THRESHOLD,
                10,
                node_config,
                remote_peers,
                1024,
                true,
                true,
            )
            .await
            .unwrap();

            let input = if id == DEALER_IDX {
                Some(&random_elements[..])
            } else {
                None
            };
            let shares = backend
                .input_slice(input, random_elements.len(), DEALER_IDX)
                .await
                .unwrap();
            info!("Input from Party {}: {:?}", id, input);

            let revealed_element = backend.reveal_slice_to_all(&shares).await.unwrap();
            info!("Revealed element for Party {}: {:?}", id, revealed_element);
            for (revealed, original) in revealed_element.iter().zip(random_elements.iter()) {
                assert_eq!(*revealed, *original);
            }

            true
        }));
    }

    // Wait for all threads to complete
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}
