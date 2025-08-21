#[repr(C, align(16))]
pub(crate) struct BlockWrapper {
    inner_block: [u8; 16],
}

extern "C" {
    pub(crate) fn new_block(inner_block: *const u8) -> *mut BlockWrapper;
    pub(crate) fn delete_block(block: *mut BlockWrapper);
}

pub struct Block {
    pub(crate) inner_block: *mut BlockWrapper,
}

impl Block {
    pub fn new(data: [u8; 16]) -> Self {
        let inner_block = unsafe { new_block(data.as_ptr()) };
        Self { inner_block }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        unsafe { delete_block(self.inner_block) };
    }
}
