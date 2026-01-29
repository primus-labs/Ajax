/// cargo run --package thfhe --example thfhe --release -- -n 3
///  cargo build --package thfhe --example thfhe --release
use algebra::Field;
use clap::{Parser, Subcommand};
use emp_bindings::generate_triples;
use libp2p::identity::{Keypair, PublicKey};
use libp2p::{Multiaddr, PeerId};
use mpc::{DNBackend, MPCBackend};
use network::p2p::NodeConfig;
use rand::SeedableRng;
use std::str::FromStr;
use thfhe::{distdec, Evaluator, Fp, KeyGen, DEFAULT_128_BITS_PARAMETERS};
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

const RING_MODULUS: u64 = Fp::MODULUS_VALUE;
const BASE_PORT: usize = 20500;

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

#[derive(Clone, Subcommand)]
enum ExecType {
    Local,
    Distributed {
        #[arg(short = 'i')]
        party_id: usize,
        private_key_path: String,
        public_key_paths: Vec<String>,
    },
}

#[derive(Parser)]
struct Cli {
    /// Number of parties participating in the protocol.
    #[arg(short = 'n')]
    n: usize,
    /// Threshold for corrupted parties.
    #[arg(short = 't')]
    t: usize,
    /// Number of triples required for the computation.
    #[arg(short = 'r')]
    triples: Option<usize>,
    /// Local execution or distributed execution.
    #[command(subcommand)]
    exec_type: ExecType,
}

#[tokio::main]
async fn main() {
    setup_tracing();
    let args = Cli::parse();
    let number_parties = args.n;
    let threshold = args.t;

    let triples_required = args.triples.unwrap_or(100);

    if number_parties < threshold / 2 {
        error!(
            "Number of parties should be greater than threshold/2, but got {} and {} respectively.",
            number_parties,
            threshold / 2
        );
        std::process::exit(1);
    }

    match args.exec_type {
        ExecType::Local => {
            // Generates the key pairs for each party to establish the secure connections between them.
            let key_pairs = (0..number_parties)
                .map(|_| Keypair::generate_ed25519())
                .collect::<Vec<_>>();

            let mut handlers = Vec::new();
            for party_id in 0..number_parties {
                let key_pairs = key_pairs.clone();
                let handler = tokio::spawn(async move {
                    // Sets the information of the remote peers.
                    let mut remote_peers = Vec::new();
                    for other_id in 0..number_parties {
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
                    let listen_addr = Multiaddr::from_str(&format!(
                        "/ip4/127.0.0.1/tcp/{}",
                        BASE_PORT + party_id
                    ))
                    .unwrap();

                    let listen_addrs = vec![listen_addr];
                    let key_pair = key_pairs[party_id].clone();

                    // Generate the node configuration
                    let node_config = NodeConfig::new(listen_addrs, key_pair);

                    thfhe(
                        party_id,
                        number_parties,
                        threshold,
                        node_config,
                        remote_peers,
                        triples_required,
                    )
                    .await;
                });

                handlers.push(handler);
            }

            for handle in handlers {
                handle.await.unwrap();
            }
        }
        ExecType::Distributed {
            party_id,
            private_key_path,
            public_key_paths,
        } => {
            // Loads the public keys from the file.
            let participants = public_key_paths
                .into_iter()
                .map(|path| {
                    let file = std::fs::read(path).unwrap();
                    PublicKey::try_decode_protobuf(&file).unwrap()
                })
                .zip(0..number_parties)
                .filter(|(_, id)| *id != party_id)
                .map(|(remote_pk, remote_party_id)| {
                    let dial_addr = Multiaddr::from_str(&format!(
                        "/ip4/127.0.0.1/tcp/{}",
                        BASE_PORT + remote_party_id
                    ))
                    .unwrap();
                    (
                        PeerId::from_public_key(&remote_pk),
                        remote_party_id,
                        vec![dial_addr],
                    )
                })
                .collect();

            let mut bytes = std::fs::read(private_key_path).unwrap();
            let local_keypair = Keypair::rsa_from_pkcs8(&mut bytes).unwrap();
            let listen_addr =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", BASE_PORT + party_id))
                    .unwrap();
            let listen_addrs = vec![listen_addr];
            let node_config = NodeConfig::new(listen_addrs, local_keypair);

            thfhe(
                party_id,
                number_parties,
                threshold,
                node_config,
                participants,
                triples_required,
            )
            .await;
        }
    }
}

async fn thfhe(
    party_id: usize,
    num_parties: usize,
    threshold: usize,
    node_config: NodeConfig,
    participants: Vec<(PeerId, usize, Vec<Multiaddr>)>,
    triples_required: usize,
) {
    let start = std::time::Instant::now();

    let rng = &mut rand::rngs::StdRng::seed_from_u64(1);

    let parameters = &DEFAULT_128_BITS_PARAMETERS;
    let lwe_params = parameters.input_lwe_params();

    // Set up the DN backend.
    let mut backend = DNBackend::<RING_MODULUS>::new(
        party_id,
        num_parties,
        threshold,
        triples_required,
        node_config,
        participants,
        parameters.ring_dimension(),
        true,
        true,
    )
    .await
    .unwrap();
    let (sk, pk, evk) = KeyGen::generate_mpc_key_pair(&mut backend, **parameters, rng).await;

    info!(
        "Party {:?} had finished keygen, NetInfo:{:?}",
        backend.party_id(),
        backend.netio.stats()
    );
    let evaluator = Evaluator::new(evk);

    let test_total_num = [1, 10];

    info!("Generating triples");
    let ips = (0..num_parties)
        .map(|_| "127.0.0.1".to_string())
        .collect::<Vec<_>>();
    let triples = generate_triples(
        party_id,
        num_parties,
        (BASE_PORT - 10500) as i32,
        &ips,
        triples_required,
    )
    .unwrap();

    backend.add_triples_z2k(triples);

    let a: u64 = 1;
    let b: u64 = 2;
    let x = pk.encrypt(a, lwe_params, rng);
    let y = pk.encrypt(b, lwe_params, rng);
    let res = evaluator.add(&x, &y);
    let public_a_single = backend
        .sends_slice_to_all_parties(Some(res.a()), res.a().len(), 0)
        .await;
    let public_b_single = backend
        .sends_slice_to_all_parties(Some(&[res.b()]), vec![res.b()].len(), 0)
        .await[0];

    for test_num in test_total_num {
        let public_a = vec![public_a_single.clone(); test_num];
        let public_b = vec![public_b_single; test_num];

        if party_id <= threshold {
            let my_sk = sk.input_lwe_secret_key.as_ref();

            let (my_dd_res, (online_duration, offline_duration)) =
                distdec(&mut backend, rng, &public_a, &public_b, my_sk).await;
            println!(
                "Party {} had finished the {}-dd-online with time {} ns,",
                party_id,
                test_num,
                online_duration.as_nanos()
            );
            println!(
                "Party {} had finished the {}-dd-offline with time {} ns,",
                party_id,
                test_num,
                offline_duration.as_nanos()
            );

            println!(
                "Party {:?} had finished {}-dd-offline, NetInfo:{:?}",
                party_id,
                test_num,
                backend.netio.stats()
            );

            if party_id == 0 {
                let my_dd_res: Vec<u64> = my_dd_res.unwrap();
                println!(
                    "(a + b )%4= {}, my party id: {}, my dd result: {:?}",
                    (a + b) % 4,
                    backend.party_id(),
                    my_dd_res[0] % 4
                );
            }
        }
    }

    println!(
        "Party {} had finished the program with time {:?}",
        party_id,
        start.elapsed()
    );
}
