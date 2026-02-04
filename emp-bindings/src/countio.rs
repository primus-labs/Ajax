use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

/// Wrapper for the network IO channel.
#[repr(C)]
pub struct CountNetIoWrapper {
    _private: [u8; 0],
}

extern "C" {
    /// Create a new network IO channel.
    fn new_count_net_io(
        address: *const c_char,
        port: c_int,
        quiet: usize,
    ) -> *mut CountNetIoWrapper;

    /// Deletes the network IO channel. This is required for the destructor to work properly.
    fn delete_count_net_io(io: *const CountNetIoWrapper);

    /// Get the number of bytes sent.
    fn count_net_io_get_bytes_sent(io: *const CountNetIoWrapper) -> usize;

    /// Get the number of bytes received.
    fn count_net_io_get_bytes_recv(io: *const CountNetIoWrapper) -> usize;

    /// Send data over the network IO channel.
    fn send_data_internal(io: *mut CountNetIoWrapper, data: *mut c_void, len: usize);

    /// Receives data from the network IO channel.
    fn recv_data_internal(io: *mut CountNetIoWrapper, data: *mut c_void, len: usize);
}

/// A network IO channel that can be used for sending and receiving data over the network. This
/// network implementation counts the number of bytes sent and received.
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

    /// Gets the number of bytes sent.
    pub fn bytes_sent(&self) -> usize {
        unsafe { count_net_io_get_bytes_sent(self.ptr) }
    }

    /// Gets the number of bytes received.
    pub fn bytes_recv(&self) -> usize {
        unsafe { count_net_io_get_bytes_recv(self.ptr) }
    }

    /// Gets a pointer to the underlying network instance.
    pub fn as_ptr(&self) -> *mut CountNetIoWrapper {
        self.ptr
    }

    /// Sends data to the other party using the network channel.
    pub fn send_data(&self, data: &mut [u64]) {
        unsafe {
            send_data_internal(self.ptr, data.as_mut_ptr() as *mut c_void, data.len() * 8);
        }
    }

    /// Receives data from the other party using the network channel.
    pub fn recv_data(&self, data: &mut [u64]) {
        unsafe {
            recv_data_internal(self.ptr, data.as_mut_ptr() as *mut c_void, data.len() * 8);
        }
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
