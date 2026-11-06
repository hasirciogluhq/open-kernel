//! Rust view of the xlang page allocator (`pmm.xl`, C ABI).

unsafe extern "C" {
    pub fn pmm_init(storage: usize, pages: usize);
    pub fn pmm_mark(start_pfn: usize, count: usize, free: u32);
    pub fn pmm_alloc() -> usize;
    pub fn pmm_alloc_pages(n: usize) -> usize;
    pub fn pmm_free_pages(phys: usize, n: usize);
    pub fn pmm_free_count() -> usize;
    #[allow(dead_code)]
    pub fn pmm_total() -> usize;
}
