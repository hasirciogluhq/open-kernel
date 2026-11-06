#ifndef KERNEL_MM_HEAP_H
#define KERNEL_MM_HEAP_H

#include <stddef.h>

/* heap.xl */
void   heap_init(size_t base, size_t size);
void  *kmalloc(size_t n);
void   kfree(void *p);
size_t heap_used(void);

/* pmm.xl */
void   pmm_init(size_t storage, size_t pages);
void   pmm_mark(size_t start_pfn, size_t count, unsigned free);
size_t pmm_alloc(void);
size_t pmm_alloc_pages(size_t n);
void   pmm_free_pages(size_t phys, size_t n);
size_t pmm_free_count(void);

#endif
