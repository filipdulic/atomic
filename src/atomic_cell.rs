use std::cell::UnsafeCell;
use std::fmt;
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{self, AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::thread;

pub struct AtomicCell<T> {
    inner: UnsafeCell<T>,
}

impl<T> AtomicCell<T> {
    pub fn new(val: T) -> AtomicCell<T> {
        AtomicCell {
            inner: UnsafeCell::new(val),
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.inner.get() }
    }

    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }

    pub fn is_lock_free() -> bool {
        atomic_is_lock_free::<T>()
    }

    pub fn set(&self, val: T) {
        if mem::needs_drop::<T>() {
            drop(self.replace(val));
        } else {
            unsafe {
                atomic_store(self.inner.get(), val, Ordering::SeqCst);
            }
        }
    }

    pub fn replace(&self, val: T) -> T {
        unsafe { atomic_swap(self.inner.get(), val, Ordering::SeqCst) }
    }
}

impl<T: Default> AtomicCell<T> {
    pub fn take(&self) -> T {
        self.replace(T::default())
    }
}

impl<T: Copy> AtomicCell<T> {
    pub fn get(&self) -> T {
        unsafe { atomic_load(self.inner.get(), Ordering::SeqCst) }
    }

    pub fn update<F>(&self, mut f: F) -> T
    where
        F: FnMut(T) -> T,
    {
        let mut current = self.get();

        loop {
            let new = f(current);

            let previous = unsafe {
                atomic_compare_and_swap(self.inner.get(), current, new, Ordering::SeqCst)
            };

            if byte_eq(&previous, &current) {
                return new;
            }

            current = previous;
        }
    }
}

impl<T: Copy + Eq> AtomicCell<T> {
    pub fn compare_and_set(&self, mut current: T, new: T) -> bool {
        loop {
            let previous = unsafe {
                atomic_compare_and_swap(self.inner.get(), current, new, Ordering::SeqCst)
            };

            if byte_eq(&previous, &current) {
                return true;
            }

            if previous != current {
                return false;
            }

            current = previous;
        }
    }
}

macro_rules! impl_arithmetic {
    ($t:ty, $atomic:ty) => {
        impl AtomicCell<$t> {
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                let a = unsafe { &*(self.inner.get() as *const $atomic) };
                a.fetch_add(val, Ordering::SeqCst).wrapping_add(val)
            }

            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                let a = unsafe { &*(self.inner.get() as *const $atomic) };
                a.fetch_sub(val, Ordering::SeqCst).wrapping_sub(val)
            }
        }
    };
    ($t:ty) => {
        impl AtomicCell<$t> {
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                self.update(|x| x.wrapping_add(val))
            }

            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                self.update(|x| x.wrapping_sub(val))
            }
        }
    };
}

#[cfg(not(feature = "nightly"))]
impl_arithmetic!(u8);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(i8);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(u16);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(i16);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(u32);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(i32);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(u64);
#[cfg(not(feature = "nightly"))]
impl_arithmetic!(i64);

#[cfg(feature = "nightly")]
impl_arithmetic!(u8, atomic::AtomicU8);
#[cfg(feature = "nightly")]
impl_arithmetic!(i8, atomic::AtomicI8);
#[cfg(feature = "nightly")]
impl_arithmetic!(u16, atomic::AtomicU16);
#[cfg(feature = "nightly")]
impl_arithmetic!(i16, atomic::AtomicI16);
#[cfg(feature = "nightly")]
impl_arithmetic!(u32, atomic::AtomicU32);
#[cfg(feature = "nightly")]
impl_arithmetic!(i32, atomic::AtomicI32);
#[cfg(feature = "nightly")]
impl_arithmetic!(u64, atomic::AtomicU64);
#[cfg(feature = "nightly")]
impl_arithmetic!(i64, atomic::AtomicI64);

impl_arithmetic!(usize, atomic::AtomicUsize);
impl_arithmetic!(isize, atomic::AtomicIsize);

impl<T: Default> Default for AtomicCell<T> {
    fn default() -> AtomicCell<T> {
        AtomicCell::new(T::default())
    }
}

impl<T: Copy + fmt::Debug> fmt::Debug for AtomicCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AtomicCell")
            .field("value", &self.get())
            .finish()
    }
}

fn byte_eq<T>(a: &T, b: &T) -> bool {
    unsafe {
        let a = slice::from_raw_parts(a as *const _ as *const u8, mem::size_of::<T>());
        let b = slice::from_raw_parts(b as *const _ as *const u8, mem::size_of::<T>());
        a == b
    }
}

struct LockGuard {
    lock: &'static AtomicBool,
}

impl Drop for LockGuard {
    #[inline]
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

#[inline]
fn lock(addr: usize) -> LockGuard {
    const LEN: usize = 499;
    const A: AtomicBool = ATOMIC_BOOL_INIT;
    static LOCKS: [AtomicBool; LEN] = [
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
        A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A, A,
    ];

    let lock = &LOCKS[addr % LEN];
    let mut step = 0usize;

    while lock.compare_and_swap(false, true, Ordering::Acquire) {
        if step < 5 {
            // do nothing
        } else if step < 10 {
            atomic::spin_loop_hint();
        } else {
            thread::yield_now();
        }
        step = step.wrapping_add(1);
    }

    LockGuard { lock }
}

macro_rules! atomic {
    (@check, $t:ty, $atomic:ty, $a:ident, $atomic_op:expr) => {
        let ok_size = mem::size_of::<$t>() == mem::size_of::<$atomic>();
        let ok_align = mem::align_of::<$t>() >= mem::align_of::<$atomic>();

        if ok_size && ok_align {
            let $a: &$atomic;
            break $atomic_op
        }
    };
    ($t:ty, $a:ident, $atomic_op:expr, $fallback_op:expr) => {
        loop {
            atomic!(@check, $t, ::std::sync::atomic::AtomicBool, $a, $atomic_op);
            atomic!(@check, $t, ::std::sync::atomic::AtomicUsize, $a, $atomic_op);

            #[cfg(feature = "nightly")]
            {
                #[cfg(target_has_atomic = "8")]
                atomic!(@check, $t, ::std::sync::atomic::AtomicU8, $a, $atomic_op);
                #[cfg(target_has_atomic = "16")]
                atomic!(@check, $t, ::std::sync::atomic::AtomicU16, $a, $atomic_op);
                #[cfg(target_has_atomic = "32")]
                atomic!(@check, $t, ::std::sync::atomic::AtomicU32, $a, $atomic_op);
                #[cfg(target_has_atomic = "64")]
                atomic!(@check, $t, ::std::sync::atomic::AtomicU64, $a, $atomic_op);
            }

            break $fallback_op
        }
    };
}

fn atomic_is_lock_free<T>() -> bool {
    atomic! { T, _a, true, false }
}

unsafe fn atomic_load<T>(dst: *mut T, order: Ordering) -> T
where
    T: Copy,
{
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            mem::transmute_copy(&a.load(order))
        },
        {
            let _lock = lock(dst as usize);
            ptr::read(dst)
        }
    }
}

unsafe fn atomic_store<T>(dst: *mut T, val: T, order: Ordering) {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            a.store(mem::transmute_copy(&val), order)
        },
        {
            let _lock = lock(dst as usize);
            ptr::write(dst, val)
        }
    }
}

unsafe fn atomic_swap<T>(dst: *mut T, val: T, order: Ordering) -> T {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            mem::transmute_copy(&a.swap(mem::transmute_copy(&val), order))
        },
        {
            let _lock = lock(dst as usize);
            ptr::replace(dst, val)
        }
    }
}

unsafe fn atomic_compare_and_swap<T>(dst: *mut T, current: T, new: T, order: Ordering) -> T
where
    T: Copy,
{
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            mem::transmute_copy(
                &a.compare_and_swap(
                    mem::transmute_copy(&current),
                    mem::transmute_copy(&new),
                    order,
                )
            )
        },
        {
            let _lock = lock(dst as usize);
            if byte_eq(&current, &new) {
                ptr::replace(dst, new)
            } else {
                ptr::read(dst)
            }
        }
    }
}
