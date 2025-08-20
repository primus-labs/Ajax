#[repr(C)]
pub struct Block;

extern "C" {
    pub fn new_block() -> *mut Block;
}
