/// Wrapper for a block of 128-bit data.
#[repr(C, align(16))]
pub(crate) struct BlockWrapper {
    inner_block: [u8; 16],
}

extern "C" {
    /// Creates a new block of 128-bit data.
    pub(crate) fn new_block(inner_block: *const u8) -> *mut BlockWrapper;
    /// Deletes the block of 128-bit data. This is required for the destructor to work properly.
    pub(crate) fn delete_block(block: *mut BlockWrapper);
}

/// API for a block of 128-bit data.
pub struct Block {
    pub(crate) inner_block: *mut BlockWrapper,
}

impl Block {
    /// Creates a new block of 128-bit data.
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
