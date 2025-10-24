use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
pub struct CountNetIoWrapper {
    _private: [u8; 0],
}

extern "C" {
    fn new_count_net_io(address: *const c_char, port: c_int, quiet: usize) -> *mut CountNetIoWrapper;
    fn delete_count_net_io(io: *const CountNetIoWrapper);
    fn count_net_io_get_bytes_sent(io: *const CountNetIoWrapper) -> usize;
    fn count_net_io_get_bytes_recv(io: *const CountNetIoWrapper) -> usize;
    fn send_data_internal(io: *mut CountNetIoWrapper, data: *mut c_char, len: usize);
    fn recv_data_internal(io: *mut CountNetIoWrapper, data: *mut c_char, len: usize);
}

#[derive(Debug, Clone)]
pub struct CountNetIo {
    pub ptr: *mut CountNetIoWrapper,
}

impl CountNetIo {
    /// Create a new CountNetIO
    pub fn new(address: Option<&str>, port: i32, quiet: bool) -> Self {
        let c_address = address.map(|s| CString::new(s).unwrap());
        let ptr = unsafe {
            new_count_net_io(
                c_address.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                port,
                if quiet { 1 } else { 0 },
            )
        };
        assert!(!ptr.is_null(), "Failed to create CountNetIO");
        Self { ptr }
    }

    pub fn bytes_sent(&self) -> usize {
        unsafe { count_net_io_get_bytes_sent(self.ptr) }
    }

    pub fn bytes_recv(&self) -> usize {
        unsafe { count_net_io_get_bytes_recv(self.ptr) }
    }

    pub fn as_ptr(&self) -> *mut CountNetIoWrapper {
        self.ptr
    }

    pub fn send_data(&self, data: &mut [u8]) {
        unsafe {
            send_data_internal(
                self.ptr,
                data.as_mut_ptr() as *mut c_char,
                data.len(),
            );
        }
    }

    pub fn recv_data(&self, data: &mut [u8]) {
        unsafe {
            recv_data_internal(
                self.ptr,
                data.as_mut_ptr() as *mut c_char,
                data.len(),
            );
        }
    }


    pub fn take_ptr(&mut self) -> *mut CountNetIoWrapper {
        // Take the pointer value
        let ptr = self.ptr;
        // Set the internal pointer to null so that Drop will not delete the C++ object.
        self.ptr = std::ptr::null_mut(); 
        ptr
    }

    /// Helper to reconstruct the struct after it was temporarily nullified by take_ptr.
    pub fn from_ptr(ptr: *mut CountNetIoWrapper) -> Self {
        Self { ptr }
    }

}

impl Drop for CountNetIo {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { delete_count_net_io(self.ptr) };
        }
    }
}

unsafe impl Send for CountNetIo {}
unsafe impl Sync for CountNetIo {}