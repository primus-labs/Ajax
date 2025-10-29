#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::constants::{PrimalLpnParameter, PrimalLpnParameterWrapper};
use crate::countio::{CountNetIo, CountNetIoWrapper};
use crate::io::{NetIo, NetIoWrapper};
use crate::utils::{Block, BlockWrapper};
use rand::Rng;
use std::ffi::{c_char, CString};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

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
    pub(crate) fn new_ole_f2k(NetIoWrapper: *mut NetIoWrapper) -> *mut OleF2kWrapper;

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
        ios: *const *mut NetIoWrapper,
        n_ios: usize,
        malicious: bool,
        run_setup: bool,
        param: *mut PrimalLpnParameterWrapper,
        pre_file: *const c_char,
    ) -> *mut FerretCotWrapper;

    pub(crate) fn delete_ferret_cot(ios: *mut FerretCotWrapper);

    pub(crate) fn new_ole_z2k(
        net_io_wrapper: *mut NetIoWrapper,
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
    pub fn new(net_io: &NetIo) -> Self {
        let inner_ole = unsafe { new_ole_f2k(net_io.inner_net_io) };
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
    pub fn new(net_io: &NetIo, cot: &FerretCot, bitlength: usize) -> Self {
        let inner_ole = unsafe { new_ole_z2k(net_io.inner_net_io, cot.inner_cot, bitlength) };
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
}

unsafe impl Send for FerretCot {}
unsafe impl Sync for FerretCot {}

impl FerretCot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        party: u32,
        threads: u32,
        ios: &mut [&mut NetIo],
        n_ios: usize,
        malicious: bool,
        run_setup: bool,
        param: &PrimalLpnParameter,
        pre_file: &str,
    ) -> Self {
        // Converts the ios into raw pointers
        let pointers: Vec<*mut NetIoWrapper> = ios.iter_mut().map(|io| io.inner_net_io).collect();
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
        Self { inner_cot }
    }
}

// impl Drop for FerretCot {
//     fn drop(&mut self) {
//         unsafe { delete_ferret_cot(self.inner_cot) };
//     }
// }

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
    // --- Time Tracking Setup ---
    let start_total = Instant::now();

    assert!(party < total_party);
    assert!(ip_list.len() == total_party);

    // --- Initialize NetIO channels ---
    let start_io = Instant::now();

    let mut ios: Vec<Option<NetIo>> = Vec::with_capacity(total_party);
    ios.resize_with(total_party, || None);

    // --- 1. Initialize NetIO channels (Phase 1: LISTENERS) ---
    // Only parties i < party will be listening on their side for the connection from party.
    for i in 0..total_party {
        if i == party {
            continue;
        }
        if i < party {
            let listen_port = (i * total_party + party) as i32 + base_port;
            ios[i] = Some(NetIo::new(None, listen_port, false));
        }
    }

    // Give OS time to bind sockets before we try to connect
    thread::sleep(std::time::Duration::from_millis(100));

    // --- 2. Initialize NetIO channels (Phase 2: CONNECTORS) ---
    for i in 0..total_party {
        if i == party {
            continue;
        }

        if i > party {
            let connect_port = (party * total_party + i) as i32 + base_port;
            ios[i] = Some(NetIo::new(Some(&ip_list[i]), connect_port, false));
        }
    }
    // Give OS time to establish connections
    thread::sleep(std::time::Duration::from_millis(100));

    let duration_io = start_io.elapsed();
    println!(
        "IO Initialization time: {} microseconds",
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
    println!(
        "Data Preparation time: {} microseconds",
        duration_data.as_micros()
    );

    // From thtfhe/triples/test/triples.cpp
    // const static int XOR = -1;
    // const static int PUBLIC = 0;
    // const static int ALICE = 1;
    // const static int BOB = 2;
    // const static PrimalLPNParameter ferret_b13 = PrimalLPNParameter(10485760, 1280, 452000, 13, 470016, 918, 32768, 9);

    thread::sleep(std::time::Duration::from_millis(200));
    let start = Instant::now();
    //-------------------------------------------------------------------------------
    // Initialize COT instances
    let ios_arc = Arc::new(Mutex::new(ios));

    let mut cots: Vec<Option<FerretCot>> = Vec::with_capacity(total_party);
    cots.resize_with(total_party, || None);
    let cots_arc = Arc::new(Mutex::new(cots));

    let mut threads = vec![];

    for i in 0..total_party {
        if i != party {
            let ios_clone: Arc<Mutex<Vec<Option<NetIo>>>> = Arc::clone(&ios_arc);
            let cots_clone: Arc<Mutex<Vec<Option<FerretCot>>>> = Arc::clone(&cots_arc);

            threads.push(thread::spawn(move || {
                let mut ios_guard = ios_clone.lock().unwrap();
                let io = ios_guard[i].as_mut().unwrap();

                let role = if i > party { 2 } else { 1 };
                let pre_file = format!("data/pre_file_{}_{}.txt", party, i);

                let cot = FerretCot::new(
                    role,
                    1,
                    &mut [io],
                    1,
                    false,
                    true,
                    &PrimalLpnParameter::new(10485760, 1280, 452000, 13, 470016, 918, 32768, 9), // ferret_b13 constant
                    &pre_file,
                );

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
    println!(
        "COT Instances Initialization time: {} microseconds",
        duration.as_micros()
    );
    // // // --- OLE computation ---
    // let start_comp = Instant::now();
    // let mut handles: Vec<std::thread::JoinHandle<(usize, Vec<u64>, NetIo, FerretCot)>> = vec![];

    // let mut tmp_out: Vec<Vec<u64>> = vec![vec![0; num_triples * 2]; total_party];
    // for i in 0..total_party {
    //     if i == party {
    //         continue;
    //     }

    //     let io_instance = ios[i].take().unwrap();
    //     let cot_instance = cots_results[i].take().unwrap();

    //     // Clone the input data for the thread
    //     let input = if i > party {
    //         a_extend_b.clone()
    //     } else {
    //         b_extend_a.clone()
    //     };
    //     let mut output = vec![0u64; num_triples_2];

    //     handles.push(thread::spawn(
    //         move || -> (usize, Vec<u64>, NetIo, FerretCot) {
    //             let io_ref = &io_instance;
    //             let cot_ref = &cot_instance;

    //             // OLE computation
    //             let ole = OleZ2k::new(io_ref, cot_ref, 64);
    //             ole.compute(&mut output, &input, num_triples << 1, MAX_BATCH_SIZE);

    //             (i, output, io_instance, cot_instance)
    //         },
    //     ));
    // }

    // // Collect OLE computation results and restore instances
    // for handle in handles.drain(..) {
    //     let (i, result, io_instance, cot_instance) = handle.join().unwrap();
    //     tmp_out[i] = result;
    //     ios[i] = Some(io_instance);
    //     cots_results[i] = Some(cot_instance);
    // }

    // for handle in handles {
    //     handle.join().unwrap();
    // }
    // // handles.clear();

    // // --- Aggregate outputs ---
    // for i in 0..total_party {
    //     if i == party {
    //         continue;
    //     }
    //     for j in 0..num_triples {
    //         out[j] = out[j]
    //             .wrapping_add(tmp_out[i][j * 2])
    //             .wrapping_add(tmp_out[i][j * 2 + 1]);
    //     }
    // }

    // let duration_comp = start_comp.elapsed();
    // println!(
    //     "Computation time: {} microseconds for {} triples",
    //     duration_comp.as_micros(),
    //     num_triples
    // );

    // // TODO: Remove this section later
    // // --- Write to file ---
    // let start_file = Instant::now();
    // let file_path = format!("data/triples_P_{}.txt", party);

    // // Ensure 'data' directory exists
    // fs::create_dir_all("data")?;
    // let mut ofile = File::create(file_path)?;
    // for i in 0..num_triples {
    //     writeln!(ofile, "a: {}, b: {}, c: {}", in_a[i], in_b[i], out[i])?;
    // }
    // let duration_file = start_file.elapsed();
    // println!(
    //     "File writing time: {} microseconds",
    //     duration_file.as_micros()
    // );

    // TODO: NetIO does not have recv_data and send_data methods.
    // TODO: it is in NetIo
    // // --- 8. Correctness Test (DEBUG only) ---
    // #[cfg(not(debug_assertions))]
    // {
    //     // Skip for released version
    // }
    // #[cfg(debug_assertions)]
    // {
    //     println!("Testing correctness...");
    //     let mut buf = vec![0u64; num_triples];
    //     let net_io_instance_0 = ios[0].as_ref().unwrap();

    //     if party == 0 {
    //         for i in 1..total_party {
    //             let net_io_instance_i = ios[i].as_ref().unwrap();

    //             // Recv a_i
    //             net_io_instance_i.recv_data(buf.as_mut_ptr() as *mut u8, num_triples * 8);
    //             for j in 0..num_triples {
    //                 in_a[j] = in_a[j].wrapping_add(buf[j]);
    //             }

    //             // Recv b_i
    //             net_io_instance_i.recv_data(buf.as_mut_ptr() as *mut u8, num_triples * 8);
    //             for j in 0..num_triples {
    //                 in_b[j] = in_b[j].wrapping_add(buf[j]);
    //             }

    //             // Recv c_i
    //             net_io_instance_i.recv_data(buf.as_mut_ptr() as *mut u8, num_triples * 8);
    //             for j in 0..num_triples {
    //                 out[j] = out[j].wrapping_add(buf[j]);
    //             }
    // }

    //         for i in 0..num_triples {
    //             if in_a[i].wrapping_mul(in_b[i]) != out[i] {
    //                 eprintln!("in_a * in_b: {} != out: {}", in_a[i].wrapping_mul(in_b[i]), out[i]);
    //                 let msg = CString::new("not correct!!").unwrap();
    //                 unsafe { error(msg.as_ptr()); } // Call C++ error function
    //                 process::exit(1);
    //             }
    //         }
    //         println!("passed");
    // } else {
    //         // Send a_i
    //         net_io_instance_0.send_data(in_a.as_ptr() as *const u8, num_triples * 8);
    //         // Send b_i
    //         net_io_instance_0.send_data(in_b.as_ptr() as *const u8, num_triples * 8);
    //         // Send c_i
    //         net_io_instance_0.send_data(out.as_ptr() as *const u8, num_triples * 8);
    // }
    // }

    // // --- 9. Communication Cost ---
    // let mut total_bytes_sent = 0;
    // let mut total_bytes_recv = 0;

    // for i in 0..total_party {
    //     if i != party {
    //         let io = ios[i].as_ref().unwrap();
    //         // Assuming NetIo has accessors to NetIo data
    //         // TODO: Implement `get_total_bytes_sent()` and `get_total_bytes_recv()` on your Rust NetIo/CountNetIo struct.
    //         // total_bytes_sent += io.get_total_bytes_sent();
    //         // total_bytes_recv += io.get_total_bytes_recv();
    //     }
    // }

    // println!("Party {} send + recv: {} bytes", party, total_bytes_sent.wrapping_add(total_bytes_recv));
    // println!("sent: {} bytes, recv: {} bytes", total_bytes_sent, total_bytes_recv);

    // let duration_total = start_total.elapsed();
    // println!("Total execution time: {} seconds", duration_total.as_secs_f64());

    // --- Convert to Vec<(a, b, c)> ---
    // let triples: Vec<(u64, u64, u64)> = (0..num_triples)
    //     .map(|i| (in_a[i], in_b[i], out[i]))
    //     .collect();

    // Ok(triples)
    Ok(vec![]) // Placeholder until the rest is uncommented
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
    const TEST_BASE_PORT: i32 = 12345;
    const TEST_NUM_TRIPLES: usize = 10;

    /// Helper function to spawn a party thread and run the triple generation.
    fn run_party(
        party_id: usize,
        total_party: usize,
        num_triples: usize,
        ip_list: Arc<Vec<String>>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            println!("--- Party {} STARTING ---", party_id);
            let start = Instant::now();

            let result =
                generate_triples(party_id, total_party, TEST_BASE_PORT, &ip_list, num_triples);

            let duration = start.elapsed();

            match result {
                Ok(triples) => {
                    println!(
                        "Party {} SUCCESS. Generated {} triples in {} ms.",
                        party_id,
                        triples.len(),
                        duration.as_millis()
                    );
                    assert_eq!(triples.len(), num_triples);
                }
                Err(e) => {
                    eprintln!("Party {} FAILED with error: {}", party_id, e);
                    // Force the thread to panic if it fails.
                    panic!("Party {} failed to generate triples.", party_id);
                }
            }
        })
    }

    #[test]
    // Note: Use `cargo test -- --nocapture` to see the print output.
    fn test_three_party_triple_generation() {
        println!("\n=======================================================");
        println!("       Starting Three-Party Triple Generation Test     ");
        println!("=======================================================\n");

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

        // Wait for all three parties to complete
        for handle in handles {
            handle.join().expect("One of the party threads panicked.");
        }
    }
}
