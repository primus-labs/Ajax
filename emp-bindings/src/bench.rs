use crate::generate_triples;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;


/// Benchmark entry point used by the bench_triples binary.
pub fn bench_triples_main() {
    use std::env;

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: bench_triples <num_parties> <num_triples> [base_port]");
        std::process::exit(1);
    }

    let num_parties: usize = args[1].parse().unwrap();
    let num_triples: usize = args[2].parse().unwrap();
    let base_port: i32 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(2000);

    tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env()) // Use RUST_LOG for filtering
            .init();

    let total_party = num_parties;

    let ip_list: Vec<String> = (0..total_party)
        .map(|_| "127.0.0.1".to_string())
        .collect();
    let ip_list = Arc::new(ip_list);

    println!("==============================================");
    println!("       Beaver Triple Benchmark (Local)        ");
    println!("==============================================");
    println!("Total Parties: {}", total_party);
    println!("Triples: {}", num_triples);
    println!("Base Port: {}", base_port);
    println!("----------------------------------------------");

    // Spawn 3 parties as threads
    let mut handles = vec![];

    for party in 0..total_party {
        let ip_list_clone = Arc::clone(&ip_list);

        handles.push(thread::spawn(move || {
            // --- Party Benchmark ---
            let start = Instant::now();

            println!("[Party {}] Starting triple generation...", party);

            let res = generate_triples(
                party,
                total_party,
                base_port,
                &ip_list_clone,
                num_triples,
            );

            let elapsed = start.elapsed();

            match res {
                Ok(triples) => {
                    let tps = (triples.len() as f64 / elapsed.as_secs_f64()) as u64;
                    println!(
                        "[Party {}] Completed: {} triples in {:.2?} ({} triples/sec)",
                        party,
                        triples.len(),
                        elapsed,
                        tps
                    );
                }
                Err(e) => {
                    eprintln!("[Party {}] FAILED: {}", party, e);
                    panic!();
                }
            }
        }));
        thread::sleep(Duration::from_millis(150));
    }

    // Wait for all parties
    for handle in handles {
        handle.join().unwrap();
    }

    println!("----------------------------------------------");
    println!("Benchmark Completed Successfully.");
}
