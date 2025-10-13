#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::constants::{PrimalLpnParameter, PrimalLpnParameterWrapper};
use crate::io::{NetIo, NetIoWrapper};
use crate::utils::{Block, BlockWrapper};
use crate::countio::{CountNetIo, CountNetIoWrapper};
use std::ffi::{c_char, CString};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use rand::Rng;

pub mod constants;
pub mod io;
pub mod utils;
pub mod countio;

#[repr(C)]
pub(crate) struct OleF2kWrapper {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct OleZ2kWrapper {
    _private: [u8; 0],
}

#[derive(Debug, Clone)]
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
        input: u64,
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

    pub fn compute(&self, input: u64, length: usize, cot_batch_size: usize) -> u64 {
        let mut result = 0;
        unsafe {
            compute_ole_z2k(self.inner_ole, &mut result, input, length, cot_batch_size);
        }
        result
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

impl Drop for FerretCot {
    fn drop(&mut self) {
        unsafe { delete_ferret_cot(self.inner_cot) };
    }
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
    
    // --- Initialize CountNetIO channels ---
    let mut ios: Vec<Option<CountNetIo>> = vec![None; total_party];
    for i in 0..total_party {
        if i == party { continue; }
        let port = (i * total_party + party) as i32 + base_port;
        if i > party {
            ios[i] = Some(CountNetIo::new(Some(&ip_list[i]), port, true));
        } else {
            ios[i] = Some(CountNetIo::new(None, (i * total_party + party) as i32 + base_port, true));
        }
    }

    // --- Generate random inputs ---
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
    let mut a_extend_b = vec![0u64; num_triples * 2];
    let mut b_extend_a = vec![0u64; num_triples * 2];

    for i in 0..num_triples {
        a_extend_b[i * 2] = in_a[i];
        a_extend_b[i * 2 + 1] = in_b[i];
        b_extend_a[i * 2] = in_b[i];
        b_extend_a[i * 2 + 1] = in_a[i];
    }

    // // --- Initialize Ferret COT instances ---
    // let mut cots: Vec<Option<FerretCotWrapper>> = vec![None; total_party];
    // let mut handles = vec![];
    // for i in 0..total_party {
    //     if i == party { continue; }
    //     let io_ptr = ios[i].as_ref().unwrap().as_ptr();
    //     handles.push(thread::spawn(move || {
    //         cots[i] = Some(FerretCotWrapper::new(io_ptr, 0, /* other params */));
    //     }));
    // }
    // for handle in handles { handle.join().unwrap(); }
    // handles.clear();

    // // --- OLE computation ---
    // let mut tmp_out: Vec<Vec<u64>> = vec![vec![0; num_triples * 2]; total_party];
    // for i in 0..total_party {
    //     if i == party { continue; }
    //     let io_ptr = ios[i].as_ref().unwrap().as_ptr();
    //     let cot_ptr = cots[i].as_ref().unwrap().as_ptr();
    //     let input = if i > party { &a_extend_b } else { &b_extend_a };
    //     let output = &mut tmp_out[i];

    //     handles.push(thread::spawn(move || {
    //         let ole = OleZ2kWrapper::new(io_ptr, cot_ptr, 64);
    //         ole.compute(output, input, num_triples * 2, MAX_BATCH_SIZE);
    //     }));
    // }
    // for handle in handles { handle.join().unwrap(); }

    // // --- Aggregate outputs ---
    // for i in 0..total_party {
    //     if i == party { continue; }
    //     for j in 0..num_triples {
    //         out[j] = out[j]
    //             .wrapping_add(tmp_out[i][j * 2])
    //             .wrapping_add(tmp_out[i][j * 2 + 1]);
    //     }
    // }

    // // --- Convert to Vec<(a, b, c)> ---
    // let triples: Vec<(u64, u64, u64)> = (0..num_triples)
    //     .map(|i| (in_a[i], in_b[i], out[i]))
    //     .collect();

    // Ok(triples)

    Ok(vec![(0, 0, 0)])
}