#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::constants::{PrimalLpnParameter, PrimalLpnParameterWrapper};
use crate::io::{NetIo, NetIoWrapper};
use crate::utils::{Block, BlockWrapper};
use std::ffi::{c_char, CString};

pub mod constants;
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
