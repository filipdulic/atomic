
use std::sync::atomic::{AtomicUsize, AtomicPtr, AtomicBool, Ordering};
use std::marker::PhantomData;
use pointer::Pointer;
use std::ops::Deref;

pub struct HazardCell<T: Pointer> {
    // `T` is just a pointer, so it is representable as a `usize`.
    inner: AtomicUsize,
    _marker: PhantomData<T>,
}

impl<T: Pointer> HazardCell<T> {
    pub fn new(val: T) -> Self {
        HazardCell {
            inner: AtomicUsize::new(val.into_raw()),
            _marker: PhantomData,
        }
    }

    pub fn into_inner(self) -> T {
        unsafe { T::from_raw(self.inner.load(Ordering::SeqCst)) }
    }

    pub fn get(&self) -> HazardGuard<T> {
        // We have to set a hazard pointer to to ThreadEntry first and only then return.

        let slot = Self::allocate_hazard_slot();

        loop {
            let inner = self.inner.load(Ordering::SeqCst);

            unsafe {
                let slot = &*slot;
                slot.store(inner, Ordering::SeqCst);
            }

            if self.inner.load(Ordering::SeqCst) == inner {
                return HazardGuard {
                    inner: inner,
                    slot: slot,
                    _marker: PhantomData,
                }
            }
        }
    }

    pub fn replace(&self, new_val: T) -> HazardGuard<T> {
        let new_raw = new_val.into_raw();
        let old_raw = self.inner.swap(new_raw, Ordering::SeqCst);

        HazardGuard {
            inner: old_raw,
            slot: 0 as HazardSlot,
            _marker: PhantomData,
        }
    }

    fn allocate_hazard_slot() -> HazardSlot {
        HARNESS.with(|harness| harness.allocate_hazard_slot())
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
            unsafe { drop(T::from_raw(self.inner.load(Ordering::SeqCst))) }
        }
    }
}

pub struct HazardGuard<T: Pointer> {
    inner: usize,
    slot: HazardSlot,
    _marker: PhantomData<T>,
}

impl<T: Pointer> Deref for HazardGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*(self.inner as *const T) }
    }
}

impl<T: Pointer> Drop for HazardGuard<T> {
    fn drop(&mut self) {
        // 1) Drop responsibility might have been transfered to us and we have either:
        //    - Transfer the responsibility to somebody else
        //    - Delete it
        // 2) Just remove hazard pointer
        //
        // 3) Pointer to slot is null, therefore we can drop right away

        unsafe {
            if self.slot as usize == 0 {
                drop(T::from_raw(self.inner))
            } else {
                let slot = &(*self.slot);

                if slot.swap(0, Ordering::SeqCst) != self.inner {
                    // Here we know that drop responsibility has been transfered to us
                    if !registry().try_transfer_drop_responsibility(self.inner) {
                        drop(T::from_raw(self.inner))
                    }
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
        // Some other thread has successfully extended Registry. 
        // It is our job now to delete `instance` we have just created.
        unsafe { drop(Box::from_raw(instance)) }
    }
}

fn registry() -> &'static Registry {
    let mut reg_ptr = REGISTRY.load(Ordering::SeqCst);

    if reg_ptr as usize == 0 {
        try_extend_registry(&REGISTRY);
        reg_ptr = REGISTRY.load(Ordering::SeqCst);
    }

    unsafe { &(*reg_ptr) }
}

impl Registry {
    fn register(&self) -> *const ThreadEntry {
        for entry in self.entries.iter() {
            if !entry.in_use.load(Ordering::SeqCst) {
                if entry.in_use.swap(true, Ordering::SeqCst) == false {
                    return entry as *const ThreadEntry;
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

    fn try_transfer_drop_responsibility(&self, ptr: usize) -> bool {
        for entry in self.entries.iter() {
            if entry.in_use.load(Ordering::SeqCst) {
                if entry.try_transfer_drop_responsibility(ptr) {
                    return true;
                }
            }
        }
        unsafe {
            let next = self.next.load(Ordering::SeqCst);

            if next as usize != 0 {
                (*(next as *const Registry)).try_transfer_drop_responsibility(ptr)
            } else {
                false
            }
        }
    }
}

type HazardSlot = *const AtomicUsize;

impl ThreadEntry {
    fn unregister(&self) {
        self.in_use.store(false, Ordering::SeqCst)
    }

    fn allocate_hazard_slot(&self) -> HazardSlot {
        for hazard in self.hazards.iter() {
            if hazard.load(Ordering::SeqCst) == 0 {
                return hazard as *const _;
            }
        }

        let mut next = self.next.load(Ordering::SeqCst);

        if next as usize == 0 {
            let new_entry = Box::into_raw(Box::new(ThreadEntry::default()));
            self.next.store(new_entry, Ordering::SeqCst);
            next = new_entry;
        }

        unsafe { (*next).allocate_hazard_slot() }
    }

    fn try_transfer_drop_responsibility(&self, ptr: usize) -> bool {
        for hazard in self.hazards.iter() {
            if hazard.load(Ordering::SeqCst) == ptr {
                hazard.store(0, Ordering::SeqCst);
                return true;
            }
        }
        return false;
    }
}

struct Harness {
    entry: *const ThreadEntry,
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

    fn allocate_hazard_slot(&self) -> HazardSlot {
        unsafe { (*self.entry).allocate_hazard_slot() }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        unsafe { (*self.entry).unregister() }
    }
}

