
use std::ptr;
use std::sync::atomic::{AtomicUsize, AtomicPtr, AtomicBool, Ordering};
use std::marker::PhantomData;
use pointer::Pointer;

struct HazardCell<T: Pointer> {
    // `T` is just a pointer, so it is representable as a `usize`.
    inner: AtomicUsize,
    _marker: PhantomData<T>,
}

impl<T: Pointer> HazardCell<T> {
    fn new(val: T) -> Self {
        HazardCell {
            inner: AtomicUsize::new(val.into_raw()),
            _marker: PhantomData,
        }
    }

    fn into_inner(self) -> T {
        unsafe { ptr::read(self.inner.load(Ordering::SeqCst) as *const T) }
    }

    fn get(&self) -> HazardGuard<T> {
        // We have to set a hazard pointer to to ThreadEntry first and only then return.

        loop {
            let inner = self.inner.load(Ordering::SeqCst);
            let (thread_entry, index) = HARNESS.with(|harness| harness.set_hazard(inner));
            if self.inner.load(Ordering::SeqCst) == inner {
                return HazardGuard {
                    inner: inner,
                    thread_entry: thread_entry,
                    index: index,
                    _marker: PhantomData,
                }
            }
        }
    }
}

unsafe impl<T: Pointer> Send for HazardCell<T> {}
unsafe impl<T: Pointer> Sync for HazardCell<T> {}

impl<T: Pointer> Drop for HazardCell<T> {
    fn drop(&mut self) {
        // 1) Either somebody is holding a reference to this element and we want to move
        //    responsibility of calling a drop(T) to them.
        // 2) Nobody is holding a reference to this element, therefore we are in charge of dropping
        //    an element.

        if !registry().try_transfer_drop_responsibility(self.inner.load(Ordering::SeqCst)) {
            let _dropper = unsafe { T::from_raw(self.inner.load(Ordering::SeqCst)) };
        }
    }
}

struct HazardGuard<T: Pointer> {
    inner: usize,
    thread_entry: *mut ThreadEntry,
    index: usize,
    _marker: PhantomData<T>,
}

impl<T: Pointer> Drop for HazardGuard<T> {
    fn drop(&mut self) {
        // 1) Drop responsibility might have been transfered to us and we have either:
        //    - Transfer the responsibility to somebody else
        //    - Delete it
        // 2) Just remove hazard pointer

        unsafe {
            if !(*self.thread_entry).try_unset_hazard(self.index, self.inner) {
                // Here we know that drop responsibility has been transfered to us
                
                if !registry().try_transfer_drop_responsibility(self.inner) {
                    let _dropper = T::from_raw(self.inner);
                }
            }
        }
    }
}

#[derive(Default)]
struct ThreadEntry {
    hazards: [AtomicUsize; 6],
    next: AtomicPtr<ThreadEntry>,
    in_use: AtomicBool,
}

#[derive(Default)]
struct Registry {
    // TODO(ibmandura): Let's use CachePadded here.
    // TODO(ibmandura): Let's find a good number instead of `out of thin air` 32.
    entries: [ThreadEntry; 32],
    next: AtomicPtr<Registry>,
}

static REGISTRY: AtomicPtr<Registry> = AtomicPtr::new(0 as *mut Registry);

fn try_extend_registry(ptr: &AtomicPtr<Registry>) {
    let instance = Box::into_raw(Box::new(Registry::default()));

    if !ptr.compare_exchange(0 as *mut Registry, instance, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        // Some other thread has successfully extended Registry. It is our job now to delete `instance` we have just created.
        let _dropper = unsafe { Box::from_raw(instance) };
    }
}

fn registry() -> &'static mut Registry {
    let mut reg_ptr = REGISTRY.load(Ordering::SeqCst);

    if reg_ptr as usize == 0 {
        try_extend_registry(&REGISTRY);
        reg_ptr = REGISTRY.load(Ordering::SeqCst);
    }

    unsafe { &mut (*reg_ptr) }
}

impl Registry {
    fn register(&mut self) -> *mut ThreadEntry {
        for idx in 0..self.entries.len() {
            let entry = &mut self.entries[idx];
            if !entry.in_use.load(Ordering::SeqCst) {
                if entry.in_use.compare_and_swap(false, true, Ordering::SeqCst) == false {
                    return entry as *mut ThreadEntry;
                }
            }
        }

        let mut next = self.next.load(Ordering::SeqCst);

        if next as usize == 0 {
            try_extend_registry(&self.next);
            next = self.next.load(Ordering::SeqCst);
        }

        unsafe { (*next).register() }
    }

    fn try_transfer_drop_responsibility(&mut self, ptr: usize) -> bool {
        for idx in 0..self.entries.len() {
            let entry = &mut self.entries[idx];
            if entry.in_use.load(Ordering::SeqCst) {
                if entry.try_transfer_drop_responsibility(ptr) {
                    return true;
                }
            }
        }
        unsafe {
            let next = self.next.load(Ordering::SeqCst);
            return if next as usize != 0 { (*(next as *mut Registry)).try_transfer_drop_responsibility(ptr) } else { false }
        }
    }
}

type HazardSlot = (*mut ThreadEntry, usize);

impl ThreadEntry {
    fn unregister(&mut self) {
        self.in_use.store(false, Ordering::SeqCst)
    }

    fn set_hazard(&mut self, ptr: usize) -> HazardSlot {
        let mut free_slot = None;

        for idx in 0..self.hazards.len() {
            let hazard = &mut self.hazards[idx];
            if hazard.load(Ordering::SeqCst) == 0 {
                hazard.store(ptr, Ordering::SeqCst);
                free_slot = Some(idx);
                break;
            }
        }

        if let Some(idx) = free_slot {
            return (self as *mut Self, idx);
        }

        let mut next = self.next.load(Ordering::SeqCst);

        if next as usize == 0 {
            let new_entry = Box::into_raw(Box::new(ThreadEntry::default()));
            self.next.store(new_entry, Ordering::SeqCst);
            next = new_entry;
        }

        unsafe { (*next).set_hazard(ptr) }
    }

    fn try_unset_hazard(&mut self, index: usize, expected_ptr: usize) -> bool {
        self.hazards[index].compare_and_swap(expected_ptr, 0, Ordering::SeqCst) == expected_ptr
    }

    fn try_transfer_drop_responsibility(&mut self, ptr: usize) -> bool {
        for hazard in self.hazards.into_iter() {
            if hazard.load(Ordering::SeqCst) == ptr {
                hazard.store(0, Ordering::SeqCst);
                return true;
            }
        }
        return false;
    }
}

struct Harness {
    entry: *mut ThreadEntry,
}

thread_local! {
    static HARNESS: Harness = Harness::new();
}

impl Harness {
    pub fn new() -> Self {
        Harness {
            entry: registry().register(),
        }
    }

    fn set_hazard(&self, ptr: usize) -> HazardSlot {
        unsafe { (*self.entry).set_hazard(ptr) }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        unsafe { (*self.entry).unregister() }
    }
}

