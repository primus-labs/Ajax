use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

#[derive(Debug, Clone)]
#[repr(C)]
pub struct CountNetIoWrapper {
    _private: [u8; 0],
}

extern "C" {
    fn new_count_net_io(address: *const c_char, port: c_int, quiet: usize) -> *mut CountNetIoWrapper;
    fn delete_count_net_io(io: *const CountNetIoWrapper);
    fn count_net_io_get_bytes_sent(io: *const CountNetIoWrapper) -> usize;
    fn count_net_io_get_bytes_recv(io: *const CountNetIoWrapper) -> usize;
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
}

impl Drop for CountNetIo {
    fn drop(&mut self) {
        unsafe { delete_count_net_io(self.ptr) };
    }
}

unsafe impl Send for CountNetIo {}
unsafe impl Sync for CountNetIo {}