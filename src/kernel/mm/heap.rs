//! Rust view of the xlang heap (`heap.xl`, C ABI).

unsafe extern "C" {
    pub fn heap_init(base: usize, size: usize);
    pub fn kmalloc(n: usize) -> *mut u8;
    pub fn kfree(p: *mut u8);
    pub fn heap_used() -> usize;
}
