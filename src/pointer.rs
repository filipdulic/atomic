
use std::sync::Arc;
use std::mem;

// A `Pointer` is just a smart pointer represented as one word.
pub unsafe trait Pointer {
    fn into_raw(self) -> usize;
    unsafe fn from_raw(raw: usize) -> Self;
}

unsafe impl<T> Pointer for Box<T> {
    fn into_raw(self) -> usize {
        unsafe { mem::transmute(self) }
    }

    unsafe fn from_raw(raw: usize) -> Self {
        unsafe { mem::transmute(raw) }
    }
}

unsafe impl<T> Pointer for Option<Box<T>> {
    fn into_raw(self) -> usize {
        unsafe { mem::transmute(self) }
    }

    unsafe fn from_raw(raw: usize) -> Self {
        unsafe { mem::transmute(raw) }
    }
}

unsafe impl<T> Pointer for Arc<T> {
    fn into_raw(self) -> usize {
        unsafe { mem::transmute(self) }
    }

    unsafe fn from_raw(raw: usize) -> Self {
        unsafe { mem::transmute(raw) }
    }
}

unsafe impl<T> Pointer for Option<Arc<T>> {
    fn into_raw(self) -> usize {
        unsafe { mem::transmute(self) }
    }

    unsafe fn from_raw(raw: usize) -> Self {
        unsafe { mem::transmute(raw)  }
    }
}

