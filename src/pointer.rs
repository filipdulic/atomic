
use std::rc::Rc;
use std::sync::Arc;

// A `Pointer` is just a smart pointer represented as one word.
pub unsafe trait Pointer {
    fn into_raw(self) -> usize;
    unsafe fn from_raw(raw: usize) -> Self;
}

unsafe impl<T> Pointer for Box<T> {
    fn into_raw(self) -> usize {
        Box::into_raw(self) as usize
    }

    unsafe fn from_raw(raw: usize) -> Self {
        Box::from_raw(raw as *mut T)
    }
}

unsafe impl<T> Pointer for Option<Box<T>> {
    fn into_raw(self) -> usize {
        self.map_or(0, |ptr| Box::into_raw(ptr) as usize)
    }

    unsafe fn from_raw(raw: usize) -> Self {
        if raw == 0 { None } else { Some(Box::from_raw(raw as *mut T)) }
    }
}

unsafe impl<T> Pointer for Arc<T> {
    fn into_raw(self) -> usize {
        Arc::into_raw(self) as usize
    }

    unsafe fn from_raw(raw: usize) -> Self {
        Arc::from_raw(raw as *mut T)
    }
}

unsafe impl<T> Pointer for Option<Arc<T>> {
    fn into_raw(self) -> usize {
        self.map_or(0, |ptr| Arc::into_raw(ptr) as usize)
    }

    unsafe fn from_raw(raw: usize) -> Self {
        if raw == 0 { None } else { Some(Arc::from_raw(raw as *mut T)) }
    }
}

