use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicUsize, AtomicPtr, AtomicBool, Ordering};
use std::thread;

// TODO: From<T>
// TODO: From<Arc<T>>
// TODO: From<Option<Arc<T>>>
// TODO: fn take(&self)

// TODO: use store/load/swap terminology?

// TODO: when transferring responsibility, CAS to 0x1, not 0x0 because we don't want another
// SharedArc to reuse the slot

pub struct AtomicArc<T> {
    // `T` is just a pointer, so it is representable as a `usize`.
    inner: AtomicUsize,
    _marker: PhantomData<Option<Arc<T>>>,
}

impl<T> AtomicArc<T> {
    pub fn new<U>(val: U) -> AtomicArc<T>
    where
        U: Into<Option<Arc<T>>>,
    {
        let raw = match val.into() {
            None => ptr::null_mut(),
            Some(val) => Arc::into_raw(val),
        };
        AtomicArc {
            inner: AtomicUsize::new(raw as usize),
            _marker: PhantomData,
        }
    }

    pub fn into_inner(self) -> Option<Arc<T>> {
        let raw = self.inner.load(Ordering::Relaxed);
        mem::forget(self);

        if raw == 0 {
            None
        } else {
            unsafe {
                Some(Arc::from_raw(raw as *const T))
            }
        }
    }

    pub fn get(&self) -> SharedArc<T> {
        // We have to set a hazard pointer to to ThreadEntry first and only then return.

        let slot = Self::allocate_hazard_slot();
        let slot = unsafe { &*slot };
        let mut inner = self.inner.load(Ordering::Relaxed);

        loop {
            if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
                // HACK(stjepang): On x86 architectures there are two different ways of executing
                // a `SeqCst` fence.
                //
                // 1. `atomic::fence(SeqCst)`, which compiles into a `mfence` instruction.
                // 2. `_.compare_and_swap(_, _, SeqCst)`, which compiles into a `lock cmpxchg`
                //    instruction.
                //
                // Both instructions have the effect of a full barrier, but benchmarks have shown
                // that the second one makes the algorithm faster in this particular case.
                let previous = slot.compare_and_swap(0, inner, Ordering::SeqCst);
                debug_assert_eq!(previous, 0);
            } else {
                slot.store(inner, Ordering::Relaxed);
                atomic::fence(Ordering::SeqCst);
            }

            let guard = SharedArc::new(inner, slot);

            let new = self.inner.load(Ordering::Relaxed);
            if new == inner {
                return guard;
            }

            inner = new;
            // `guard` gets dropped, potentially destroying the object.
        }
    }

    pub fn replace<U>(&self, val: U) -> SharedArc<T>
    where
        U: Into<Option<Arc<T>>>,
    {
        let new = match val.into() {
            None => 0,
            Some(val) => Arc::into_raw(val) as usize,
        };
        let old = self.inner.swap(new, Ordering::SeqCst);
        SharedArc::new(old, ptr::null())
    }

    pub fn set<U>(&self, val: U)
    where
        U: Into<Option<Arc<T>>>,
    {
        self.replace(val.into());
    }

    // TODO: turn `current` and `new` into `impl ArcArgument<T>`
    pub fn compare_and_set<U>(&self, current: &SharedArc<T>, new: U) -> Result<(), Option<Arc<T>>>
    where
        U: Into<Option<Arc<T>>>,
    {
        let new = match new.into() {
            None => 0,
            Some(val) => Arc::into_raw(val) as usize,
        };
        let old = current.inner;

        if self.inner.compare_and_swap(old, new, Ordering::SeqCst) == old {
            drop(SharedArc::<T>::new(old, ptr::null()));
            Ok(())
        } else {
            if new == 0 {
                Err(None)
            } else {
                unsafe {
                    Err(Some(Arc::from_raw(new as *const T)))
                }
            }
        }
    }

    #[inline]
    fn allocate_hazard_slot() -> HazardSlot {
        HARNESS.with(|harness| harness.allocate_hazard_slot())
    }
}

unsafe impl<T: Send + Sync> Send for AtomicArc<T> {}
unsafe impl<T: Send + Sync> Sync for AtomicArc<T> {}

impl<T> Drop for AtomicArc<T> {
    fn drop(&mut self) {
        // 1) Either somebody is holding a reference to this element and we want to move
        //    responsibility of calling a drop(T) to them.
        // 2) Nobody is holding a reference to this element, therefore we are in charge of dropping
        //    an element.

        let raw = self.inner.load(Ordering::Relaxed);

        if !registry().try_transfer_drop_responsibility(raw) {
            if raw != 0 {
                unsafe {
                    drop(Arc::from_raw(raw as *const T));
                }
            }
        }
    }
}

pub struct SharedArc<T> {
    inner: usize,
    slot: HazardSlot,
    _marker: PhantomData<Option<Arc<T>>>,
}

impl<T> SharedArc<T> {
    fn new(inner: usize, slot: HazardSlot) -> Self {
        SharedArc {
            inner: inner,
            slot: slot,
            _marker: PhantomData,
        }
    }

    // TODO: public function from Option<Arc<T>> or whatever

    pub fn clone_inner(&self) -> Option<Arc<T>> {
        let val = if self.inner == 0 {
            None
        } else {
            unsafe { Some(Arc::from_raw(self.inner as *const T)) }
        };
        let new = val.clone();
        mem::forget(val);
        new
    }

    pub fn as_ref(&self) -> Option<&Arc<T>> {
        if self.inner == 0 {
            None
        } else {
            unsafe {
                Some(mem::transmute::<&usize, &Arc<T>>(&self.inner))
            }
        }
    }

    // pub fn wait_unwrap(this: SharedArc<T>) -> Option<T> {
    //     if this.inner == 0 {
    //         None
    //     } else {
    //         let val = unsafe { Arc::from_raw(this.inner as *const T) };
    //
    //         loop {
    //             match Arc::try_unwrap(val) {
    //                 Ok(t) => return Some(t),
    //                 Err(v) => val = v,
    //             }
    //
    //             thread::yield_now();
    //         }
    //     }
    // }

    // TODO: pub fn as_inner()?
}

impl<T> Drop for SharedArc<T> {
    #[inline]
    fn drop(&mut self) {
        // 1) Drop responsibility might have been transfered to us and we have either:
        //    - Transfer the responsibility to somebody else
        //    - Delete it
        // 2) Just remove hazard pointer
        //
        // 3) Pointer to slot is null, therefore we can try to drop right away

        unsafe {
            if self.slot.is_null() {
                if !registry().try_transfer_drop_responsibility(self.inner) {
                    drop(Arc::from_raw(self.inner as *const T));
                }
            } else {
                let slot = &(*self.slot);

                if slot.swap(0, Ordering::SeqCst) != self.inner {
                    // Here we know that drop responsibility has been transfered to us
                    if !registry().try_transfer_drop_responsibility(self.inner) {
                        drop(Arc::from_raw(self.inner as *const T));
                    }
                }
            }
        }
    }
}

// impl<T> From<T> for SharedArc<T> {
//     fn from(val: T) -> SharedArc<T> {
//         unimplemented!()
//     }
// }

// impl<T> From<Arc<T>> for SharedArc<T> {
//     fn from(val: Arc<T>) -> SharedArc<T> {
//         unimplemented!()
//     }
// }

impl<T> From<T> for SharedArc<T>
where
    T: Into<Option<Arc<T>>>,
{
    fn from(val: T) -> SharedArc<T> {
        let raw = match val.into() {
            None => 0,
            Some(val) => Arc::into_raw(val) as usize,
        };
        SharedArc::new(raw, ptr::null())
    }
}

impl<T> Into<Option<Arc<T>>> for SharedArc<T> {
    fn into(self) -> Option<Arc<T>> {
        self.clone_inner()
    }
}

impl<'a, T> Into<Option<Arc<T>>> for &'a SharedArc<T> {
    fn into(self) -> Option<Arc<T>> {
        self.clone_inner()
    }
}

impl<'a, T> Into<Option<&'a Arc<T>>> for &'a SharedArc<T> {
    fn into(self) -> Option<&'a Arc<T>> {
        self.as_ref()
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

    if reg_ptr.is_null() {
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

        if next.is_null() {
            try_extend_registry(&self.next);
            next = self.next.load(Ordering::SeqCst);
        }

        unsafe { (*next).register() }
    }

    #[cold]
    fn try_transfer_drop_responsibility(&self, ptr: usize) -> bool {
        ::std::sync::atomic::fence(Ordering::SeqCst);

        for entry in self.entries.iter() {
            if entry.in_use.load(Ordering::SeqCst) {
                if entry.try_transfer_drop_responsibility(ptr) {
                    return true;
                }
            }
        }
        unsafe {
            let next = self.next.load(Ordering::SeqCst);

            if !next.is_null() {
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

    #[inline]
    fn allocate_hazard_slot(&self) -> HazardSlot {
        for hazard in self.hazards.iter() {
            if hazard.load(Ordering::Relaxed) == 0 {
                return hazard as *const _;
            }
        }

        let mut next = self.next.load(Ordering::SeqCst);

        if next.is_null() {
            let new_entry = Box::into_raw(Box::new(ThreadEntry::default()));
            self.next.store(new_entry, Ordering::SeqCst);
            next = new_entry;
        }

        unsafe { (*next).allocate_hazard_slot() }
    }

    fn try_transfer_drop_responsibility(&self, ptr: usize) -> bool {
        for hazard in self.hazards.iter() {
            if hazard.load(Ordering::SeqCst) == ptr {
                if hazard.compare_and_swap(ptr, 0, Ordering::SeqCst) == ptr {
                    return true;
                }
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

    #[inline]
    fn allocate_hazard_slot(&self) -> HazardSlot {
        unsafe { (*self.entry).allocate_hazard_slot() }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        unsafe { (*self.entry).unregister() }
    }
}

