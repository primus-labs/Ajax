use std::ffi::{c_char, CString};

#[repr(C)]
pub(crate) struct NetIoWrapper {
    _private: [u8; 0],
}

extern "C" {
    pub(crate) fn new_net_io(address: *const c_char, port: i32, quiet: usize) -> *mut NetIoWrapper;
    pub(crate) fn delete_net_io(io: *mut NetIoWrapper);
}

pub struct NetIo {
    pub(crate) inner_net_io: *mut NetIoWrapper,
}

impl NetIo {
    pub fn new(address: &str, port: i32, quiet: bool) -> Self {
        let c_address = CString::new(address).unwrap();
        let quiet_int = if quiet { 1 } else { 0 };
        let ptr = unsafe { new_net_io(c_address.as_ptr(), port, quiet_int) };
        Self { inner_net_io: ptr }
    }
}

impl Drop for NetIo {
    fn drop(&mut self) {
        unsafe {
            delete_net_io(self.inner_net_io);
        }
    }
}
