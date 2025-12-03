#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::constants::{PrimalLpnParameter, PrimalLpnParameterWrapper};
use crate::countio::{CountNetIo, CountNetIoWrapper};
use crate::utils::{Block, BlockWrapper};
use rand::Rng;
use std::ffi::{c_char, CString};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

#[allow(unused_imports)]
use tracing_subscriber::{fmt, EnvFilter};

pub mod constants;
pub mod countio;
pub mod io;
pub mod utils;

#[repr(C)]
pub(crate) struct OleF2kWrapper {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct OleZ2kWrapper {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct FerretCotWrapper {
    _private: [u8; 0],
}

extern "C" {
    pub(crate) fn new_ole_f2k(CountNetIoWrapper: *mut CountNetIoWrapper) -> *mut OleF2kWrapper;

    pub(crate) fn inner_prod_ole_f2k(
        ole: *mut OleF2kWrapper,
        res: *mut BlockWrapper,
        a: *mut BlockWrapper,
        b: *mut BlockWrapper,
        sz: i32,
    );

    pub(crate) fn compute_ole_f2k(
        ole: *mut OleF2kWrapper,
        out: *mut BlockWrapper,
        input: *mut BlockWrapper,
        length: i32,
    );

    pub(crate) fn delete_ole_f2k(ole: *mut OleF2kWrapper);

    pub(crate) fn new_ferret_cot(
        party: u32,
        threads: u32,
        ios: *const *mut CountNetIoWrapper,
        n_ios: usize,
        malicious: bool,
        run_setup: bool,
        param: *mut PrimalLpnParameterWrapper,
        pre_file: *const c_char,
    ) -> *mut FerretCotWrapper;

    pub(crate) fn delete_ferret_cot(ios: *mut FerretCotWrapper);

    pub(crate) fn new_ole_z2k(
        net_io_wrapper: *mut CountNetIoWrapper,
        cot: *mut FerretCotWrapper,
        bitlength: usize,
    ) -> *mut OleZ2kWrapper;

    pub(crate) fn delete_ole_z2k(net_io_wrapper: *mut OleZ2kWrapper);

    pub(crate) fn compute_ole_z2k(
        ole: *mut OleZ2kWrapper,
        out: *mut u64,
        input: *const u64,
        length: usize,
        cot_batch_size: usize,
    );
}

pub struct OleF2k {
    inner_ole: *mut OleF2kWrapper,
}

impl OleF2k {
    pub fn new(net_io: &CountNetIo) -> Self {
        let inner_ole = unsafe { new_ole_f2k(net_io.ptr) };
        Self { inner_ole }
    }

    pub fn inner_product(&self, res: &mut Block, a: &Block, b: &Block, sz: i32) {
        unsafe {
            inner_prod_ole_f2k(
                self.inner_ole,
                res.inner_block,
                a.inner_block,
                b.inner_block,
                sz,
            );
        }
    }

    pub fn compute(&mut self, input: &Block, length: i32) -> Block {
        let output = Block::new([0; 16]);
        unsafe {
            compute_ole_f2k(
                self.inner_ole,
                output.inner_block,
                input.inner_block,
                length,
            )
        }
        output
    }
}

impl Drop for OleF2k {
    fn drop(&mut self) {
        unsafe { delete_ole_f2k(self.inner_ole) };
    }
}

pub struct OleZ2k {
    inner_ole: *mut OleZ2kWrapper,
}

impl OleZ2k {
    pub fn new(net_io: &CountNetIo, cot: &FerretCot, bitlength: usize) -> Self {
        let inner_ole = unsafe { new_ole_z2k(net_io.ptr, cot.inner_cot, bitlength) };
        Self { inner_ole }
    }

    pub fn compute(&self, out: &mut [u64], input: &[u64], length: usize, cot_batch_size: usize) {
        unsafe {
            compute_ole_z2k(
                self.inner_ole,
                out.as_mut_ptr(),
                input.as_ptr(),
                length,
                cot_batch_size,
            );
        }
    }
}

impl Drop for OleZ2k {
    fn drop(&mut self) {
        unsafe { delete_ole_z2k(self.inner_ole) };
    }
}

pub struct FerretCot {
    inner_cot: *mut FerretCotWrapper,
    // Keep these alive as long as inner_cot is alive
    _param: PrimalLpnParameter,
    _pre_file: CString,
}

unsafe impl Send for FerretCot {}
unsafe impl Sync for FerretCot {}

impl FerretCot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        party: u32,
        threads: u32,
        ios: &mut [&mut CountNetIo],
        n_ios: usize,
        malicious: bool,
        run_setup: bool,
        param: PrimalLpnParameter,
        pre_file: String,
    ) -> Self {
        // Converts the ios into raw pointers
        let pointers: Vec<*mut CountNetIoWrapper> = ios.iter_mut().map(|io| io.ptr).collect();
        let ios_raw_ptr = pointers.as_ptr();

        let pre_file_c = CString::new(pre_file).unwrap();
        let inner_cot = unsafe {
            new_ferret_cot(
                party,
                threads,
                ios_raw_ptr,
                n_ios,
                malicious,
                run_setup,
                param.param,
                pre_file_c.as_ptr(),
            )
        };
        Self {
            inner_cot,
            _param: param,
            _pre_file: pre_file_c,
        }
    }
}

impl Drop for FerretCot {
    fn drop(&mut self) {
        unsafe { delete_ferret_cot(self.inner_cot) };
    }
}

/// Reads the IP list from a file.
pub fn read_ip_list(filename: &str, total_party: usize) -> Result<Vec<String>, std::io::Error> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let ip_list: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .take(total_party)
        .collect();
    Ok(ip_list)
}

const MAX_BATCH_SIZE: usize = 100_000;

/// Generate `num_triples` Beaver triples
pub fn generate_triples(
    party: usize,
    total_party: usize,
    base_port: i32,
    ip_list: &[String],
    num_triples: usize,
) -> Result<Vec<(u64, u64, u64)>, Box<dyn std::error::Error>> {
    assert!(party < total_party);
    assert!(ip_list.len() == total_party);

    // Initialize CountNetIO channels
    let start_io = Instant::now();

    let mut ios: Vec<Option<CountNetIo>> = Vec::with_capacity(total_party);
    ios.resize_with(total_party, || None);

    // Initialize CountNetIO channels (Phase 1: LISTENERS)
    // Only parties i < party will be listening on their side for the connection from party.
    for i in 0..total_party {
        if i == party {
            continue;
        }
        if i < party {
            let listen_port = (i * total_party + party) as i32 + base_port;
            ios[i] = Some(CountNetIo::new(None, listen_port, true));
        }
    }

    // Initialize CountNetIO channels (Phase 2: CONNECTORS)
    for i in 0..total_party {
        if i == party {
            continue;
        }

        if i > party {
            let connect_port = (party * total_party + i) as i32 + base_port;
            ios[i] = Some(CountNetIo::new(Some(&ip_list[i]), connect_port, true));
        }
    }

    let duration_io = start_io.elapsed();
    tracing::info!(
        "IO Initialization complete for party {}. Time taken: {} microseconds",
        party,
        duration_io.as_micros()
    );

    // --- Generate random inputs ---
    let start_data = Instant::now();

    let mut rng = rand::thread_rng();
    let mut in_a = vec![0u64; num_triples];
    let mut in_b = vec![0u64; num_triples];

    for i in 0..num_triples {
        in_a[i] = rng.gen();
        in_b[i] = rng.gen();
    }

    // --- Local output ---
    let mut out = vec![0u64; num_triples];
    for i in 0..num_triples {
        out[i] = in_a[i].wrapping_mul(in_b[i]);
    }

    // --- Prepare extended vectors ---
    let num_triples_2 = num_triples * 2;

    let mut a_extend_b = vec![0u64; num_triples_2];
    let mut b_extend_a = vec![0u64; num_triples_2];

    for i in 0..num_triples {
        a_extend_b[i * 2] = in_a[i];
        a_extend_b[i * 2 + 1] = in_b[i];
        b_extend_a[i * 2] = in_b[i];
        b_extend_a[i * 2 + 1] = in_a[i];
    }

    let duration_data = start_data.elapsed();
    tracing::info!(
        "Data preparation complete for party {}. Time taken: {} microseconds",
        party,
        duration_data.as_micros()
    );

    // From thtfhe/triples/test/triples.cpp
    // const static int XOR = -1;
    // const static int PUBLIC = 0;
    // const static int ALICE = 1;
    // const static int BOB = 2;
    // const static PrimalLPNParameter ferret_b13 = PrimalLPNParameter(10485760, 1280, 452000, 13, 470016, 918, 32768, 9);

    let start = Instant::now();

    // Initialize COT instances
    let ios_arc = Arc::new(Mutex::new(ios));

    let mut cots: Vec<Option<FerretCot>> = Vec::with_capacity(total_party);
    cots.resize_with(total_party, || None);
    let cots_arc = Arc::new(Mutex::new(cots));

    let mut threads = vec![];

    for i in 0..total_party {
        if i != party {
            let ios_clone: Arc<Mutex<Vec<Option<CountNetIo>>>> = Arc::clone(&ios_arc);
            let cots_clone: Arc<Mutex<Vec<Option<FerretCot>>>> = Arc::clone(&cots_arc);
            let _ = std::fs::create_dir_all("data");
            threads.push(thread::spawn(move || {
                let mut ios_guard = ios_clone.lock().unwrap();
                let io = ios_guard[i].as_mut().unwrap();

                let role = if i > party { 2 } else { 1 };
                let params =
                    PrimalLpnParameter::new(10485760, 1280, 452000, 13, 470016, 918, 32768, 9);
                let pre_file = format!("data/pre_file_{}_{}.txt", party, i);

                let cot = FerretCot::new(role, 1, &mut [io], 1, false, true, params, pre_file);

                drop(ios_guard);

                let mut cots_guard = cots_clone.lock().unwrap();
                cots_guard[i] = Some(cot);
            }));
        }
    }

    for t in threads {
        t.join().unwrap();
    }

    let duration = start.elapsed();
    tracing::info!(
        "COT Initialization complete for party {}. Time taken: {} microseconds",
        party,
        duration.as_micros()
    );

    {
        let cots_guard = cots_arc.lock().unwrap();
        for (i, cot_opt) in cots_guard.iter().enumerate() {
            if i == party {
                continue;
            }
            assert!(cot_opt.is_some(), "COT for party {} is still None!", i);
        }
    }

    // --- OLE computation ---
    let start_comp = Instant::now();
    let mut handles: Vec<std::thread::JoinHandle<(usize, Vec<u64>)>> = vec![];

    let mut tmp_out: Vec<Vec<u64>> = vec![vec![0; num_triples * 2]; total_party];
    for i in 0..total_party {
        if i == party {
            continue;
        }

        let ios_clone = Arc::clone(&ios_arc);
        let cots_clone = Arc::clone(&cots_arc);

        // Clone input for the thread
        let input = if i > party {
            a_extend_b.clone()
        } else {
            b_extend_a.clone()
        };

        let mut output = vec![0u64; num_triples * 2];

        handles.push(thread::spawn(move || -> (usize, Vec<u64>) {
            // lock CountNetIo and Cot
            let mut ios_guard = ios_clone.lock().unwrap();
            let mut cots_guard = cots_clone.lock().unwrap();

            let io_instance = ios_guard[i].as_mut().unwrap();
            let cot_instance = cots_guard[i].as_mut().unwrap();

            if io_instance.ptr.is_null() {
                panic!("Null CountNetIo pointer for party {party}");
            }
            if cot_instance.inner_cot.is_null() {
                panic!("Null FerretCot pointer for party {party}");
            }

            // OLE computation
            let ole = OleZ2k::new(io_instance, cot_instance, 64);

            ole.compute(&mut output, &input, num_triples << 1, MAX_BATCH_SIZE);

            drop(cots_guard);
            drop(ios_guard);

            (i, output)
        }));
    }

    // Collect OLE computation results
    for handle in handles {
        let (i, result) = handle.join().unwrap();
        tmp_out[i] = result;
    }

    // --- Aggregate outputs ---
    for i in 0..total_party {
        if i == party {
            continue;
        }
        for j in 0..num_triples {
            out[j] = out[j]
                .wrapping_add(tmp_out[i][j * 2])
                .wrapping_add(tmp_out[i][j * 2 + 1]);
        }
    }

    let duration_comp = start_comp.elapsed();
    tracing::info!(
        "Computation complete for party {}. Time taken: {} microseconds",
        party,
        duration_comp.as_micros()
    );

    // TODO: Remove this section later
    // --- Write to file ---
    let start_file = Instant::now();
    let file_path = format!("data/triples_P_{}.txt", party);

    // Ensure 'data' directory exists
    fs::create_dir_all("data")?;
    let mut ofile = File::create(file_path)?;
    for i in 0..num_triples {
        writeln!(ofile, "a: {}, b: {}, c: {}", in_a[i], in_b[i], out[i])?;
    }
    let duration_file = start_file.elapsed();
    tracing::info!(
        "File writing complete for party {}. Time taken: {} microseconds",
        party,
        duration_file.as_micros()
    );

    // --- Convert to Vec<(a, b, c)> ---
    let triples: Vec<(u64, u64, u64)> = (0..num_triples)
        .map(|i| (in_a[i], in_b[i], out[i]))
        .collect();

    // optional verification phase
    #[cfg(feature = "verify")]
    {
        tracing::info!("Starting correctness verification phase...");
        let start_verify = Instant::now();

        if party == 0 {
            let mut buf = vec![0u64; num_triples];
            for i in 1..total_party {
                let ios_guard = ios_arc.lock().unwrap();
                let io = ios_guard[i].as_ref().unwrap();

                // recv a_i
                io.recv_data(buf.as_mut());
                for j in 0..num_triples {
                    in_a[j] = in_a[j].wrapping_add(buf[j]);
                }

                // recv b_i
                io.recv_data(buf.as_mut());
                for j in 0..num_triples {
                    in_b[j] = in_b[j].wrapping_add(buf[j]);
                }

                // recv c_i
                io.recv_data(buf.as_mut());
                for j in 0..num_triples {
                    out[j] = out[j].wrapping_add(buf[j]);
                }
            }

            let mask: u128 = (1u128 << 64) - 1;
            let mut failures = 0usize;
            for i in 0..num_triples {
                let a = (in_a[i] as u128) & mask;
                let b = (in_b[i] as u128) & mask;
                let c = (out[i] as u128) & mask;
                let ab = (a * b) & mask;
                if ab != c {
                    tracing::error!(
                        "triple[{}]: (a*b)={} != c={} (a={}, b={})",
                        i,
                        ab as u64,
                        c as u64,
                        a as u64,
                        b as u64
                    );
                    failures += 1;
                }
            }

            if failures > 0 {
                tracing::error!(
                    "Correctness failed! {} mismatched triples out of {}.",
                    failures,
                    num_triples
                );
                panic!("Verification failed!");
            } else {
                tracing::info!("All {} triples verified correctly!", num_triples);
            }
        } else {
            // Non-zero parties send their shares to Party 0
            let ios_guard = ios_arc.lock().unwrap();
            let io0 = ios_guard[0].as_ref().unwrap();

            io0.send_data(in_a.as_mut());
            io0.send_data(in_b.as_mut());
            io0.send_data(out.as_mut());
            tracing::info!("Party {} sent shares to Party 0.", party);
        }

        // Communication Cost
        let mut total_sent = 0;
        let mut total_recv = 0;
        for i in 0..total_party {
            if i != party {
                let ios_guard = ios_arc.lock().unwrap();
                let io = ios_guard[i].as_ref().unwrap();
                total_sent += io.bytes_sent();
                total_recv += io.bytes_recv();
            }
        }

        tracing::info!(
            "Verification phase complete. Time taken: {} microseconds",
            start_verify.elapsed().as_micros()
        );
        tracing::info!(
            "Communication stats: sent={} bytes, received={} bytes, total={} bytes",
            total_sent,
            total_recv,
            total_sent + total_recv
        );
    }

    drop(cots_arc.lock().unwrap());
    drop(ios_arc.lock().unwrap());
    Ok(triples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    // A mock implementation for reading IP addresses for testing purposes.
    fn mock_read_ip_list(total_party: usize) -> Vec<String> {
        // Use loopback address for all parties to keep testing local.
        (0..total_party).map(|_| "127.0.0.1".to_string()).collect()
    }

    // Parameters for a 3-party test run
    const TEST_TOTAL_PARTY: usize = 3;
    const TEST_BASE_PORT: i32 = 4000;
    const TEST_NUM_TRIPLES: usize = 10000;

    /// Helper function to spawn a party thread and run the triple generation.
    fn run_party(
        party_id: usize,
        total_party: usize,
        num_triples: usize,
        ip_list: Arc<Vec<String>>,
    ) -> thread::JoinHandle<Vec<(u64, u64, u64)>> {
        thread::spawn(move || {
            tracing::info!("Party {} STARTING", party_id);
            let start = Instant::now();

            let result =
                generate_triples(party_id, total_party, TEST_BASE_PORT, &ip_list, num_triples);

            let duration = start.elapsed();

            match result {
                Ok(triples) => {
                    tracing::info!(
                        "Party {} generated {} triples in {} microseconds.",
                        party_id,
                        triples.len(),
                        duration.as_micros()
                    );
                    assert_eq!(triples.len(), num_triples);
                    triples
                }
                Err(e) => {
                    tracing::error!("Party {} FAILED with error: {}", party_id, e);
                    panic!("Party {} failed to generate triples.", party_id);
                }
            }
        })
    }

    /// Aggregates per-party triples and verifies correctness of a*b = c (mod 2^64).
    fn verify_triples(all_triples: Vec<Vec<(u64, u64, u64)>>) {
        let total_party = all_triples.len();
        let num_triples = all_triples[0].len();
        let mask = (1u128 << 64) - 1;
        let mut failures = 0;

        for i in 0..num_triples {
            let mut a_sum: u128 = 0;
            let mut b_sum: u128 = 0;
            let mut c_sum: u128 = 0;

            for p in 0..total_party {
                let (a, b, c) = all_triples[p][i];
                a_sum += a as u128;
                b_sum += b as u128;
                c_sum += c as u128;
            }

            let a = a_sum & mask;
            let b = b_sum & mask;
            let c = c_sum & mask;
            let ab = (a * b) & mask;

            // check non-zero values
            assert!(a != 0);
            assert!(b != 0);
            assert!(c != 0);

            if ab != c {
                tracing::error!(
                    "Mismatch at index {}: a*b={} != c={} (a={}, b={})",
                    i,
                    ab,
                    c,
                    a,
                    b
                );
                failures += 1;
            }
        }

        if failures == 0 {
            tracing::info!("All {} triples verified correctly!", num_triples);
        } else {
            tracing::error!("{} triple mismatches found!", failures);
            panic!("{} triple mismatches found!", failures);
        }
    }

    #[test]
    // Note: Use `cargo test -- --nocapture` to see the print output.
    fn test_three_party_triple_generation() {
        // Set up global tracing subscriber (logger)
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env()) // Use RUST_LOG for filtering
            .init();
        tracing::info!("-------Starting Three-Party Triple Generation Test----------");

        let ip_list = Arc::new(mock_read_ip_list(TEST_TOTAL_PARTY));
        let num_triples = TEST_NUM_TRIPLES;

        let mut handles = vec![];

        // 1. Party 0
        let h0 = run_party(0, TEST_TOTAL_PARTY, num_triples, Arc::clone(&ip_list));
        handles.push(h0);

        // Add a small delay for P0 to start listening
        thread::sleep(std::time::Duration::from_millis(100));

        // 2. Party 1
        let h1 = run_party(1, TEST_TOTAL_PARTY, num_triples, Arc::clone(&ip_list));
        handles.push(h1);

        // Add a small delay for P1 to establish its connections
        thread::sleep(std::time::Duration::from_millis(100));

        // 3. Party 2
        let h2 = run_party(2, TEST_TOTAL_PARTY, num_triples, Arc::clone(&ip_list));
        handles.push(h2);

        // Collect results
        let mut all_triples = Vec::with_capacity(TEST_TOTAL_PARTY);
        for (i, handle) in handles.into_iter().enumerate() {
            let triples = handle
                .join()
                .unwrap_or_else(|_| panic!("Party {} thread panicked", i));
            all_triples.push(triples);
        }

        // Verify correctness directly from returned triples
        verify_triples(all_triples);
    }
}
