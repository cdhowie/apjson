use std::alloc::GlobalAlloc;

use pyo3::ffi::{PyMem_Free, PyMem_Malloc, PyMem_Realloc};

struct PyMemAllocator;

unsafe impl GlobalAlloc for PyMemAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        unsafe { PyMem_Malloc(layout.size()).cast() }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
        unsafe {
            PyMem_Free(ptr.cast());
        }
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        _layout: std::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        unsafe { PyMem_Realloc(ptr.cast(), new_size).cast() }
    }
}

#[global_allocator]
static GLOBAL: PyMemAllocator = PyMemAllocator;
