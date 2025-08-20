#[repr(C)]
pub struct NetIo;

#[repr(C)]
pub struct M128i;

extern "C" {
    pub fn new_net_io() -> *mut NetIo;
    pub fn new_m128i() -> *mut M128i;
}
