use std::cell::UnsafeCell;
use std::fmt;
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{self, AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::thread;

pub struct AtomicCell<T> {
    value: UnsafeCell<T>,
}

impl<T> AtomicCell<T> {
    /// Creates a new atomic cell initialized with `val`.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    /// ```
    pub fn new(val: T) -> AtomicCell<T> {
        AtomicCell {
            value: UnsafeCell::new(val),
        }
    }

    /// Returns a raw pointer to the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    /// let ptr = a.as_ptr();
    /// ```
    pub fn as_ptr(&self) -> *mut T {
        self.value.get()
    }

    /// Returns a mutable reference to the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let mut a = AtomicCell::new(7);
    /// *a.get_mut() += 1;
    ///
    /// assert_eq!(a.get(), 8);
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }

    /// Unwraps the atomic cell and returns its inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let mut a = AtomicCell::new(7);
    /// let v = a.into_inner();
    ///
    /// assert_eq!(v, 7);
    /// ```
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Returns `true` if operations on values of this type are lock-free.
    ///
    /// If the compiler or platform don't support the necessary atomic instructions, `AtomicCell`
    /// will use global locks on every potentially concurrent atomic operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// // This type is internally represented as `AtomicUsize` so we can just use atomic
    /// // operations provided by it.
    /// assert_eq!(AtomicCell::<usize>::is_lock_free(), true);
    ///
    /// // A wrapper struct around `bool`.
    /// struct Foo {
    ///     bar: bool,
    /// }
    /// // `AtomicCell<Foo>` will be internally represented as `AtomicBool`.
    /// assert_eq!(AtomicCell::<Foo>::is_lock_free(), true);
    ///
    /// // Operations on zero-sized types are always lock-free.
    /// assert_eq!(AtomicCell::<()>::is_lock_free(), true);
    ///
    /// // Very large types cannot be represented as any of the standard atomic types, so atomic
    /// // operations on them will have to use global locks for synchronization.
    /// assert_eq!(AtomicCell::<[u8; 1000]>::is_lock_free(), false);
    /// ```
    pub fn is_lock_free() -> bool {
        atomic_is_lock_free::<T>()
    }

    /// Stores `val` into the atomic cell.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.get(), 7);
    /// a.set(8);
    /// assert_eq!(a.get(), 8);
    /// ```
    pub fn set(&self, val: T) {
        if mem::needs_drop::<T>() {
            drop(self.replace(val));
        } else {
            unsafe {
                atomic_store(self.value.get(), val);
            }
        }
    }

    /// Stores `val` into the atomic cell and returns the previous value.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.get(), 7);
    /// assert_eq!(a.replace(8), 7);
    /// assert_eq!(a.get(), 8);
    /// ```
    pub fn replace(&self, val: T) -> T {
        unsafe { atomic_swap(self.value.get(), val) }
    }
}

impl<T: Default> AtomicCell<T> {
    /// Takes the inner value and replaces it with `T::default()`.
    ///
    /// Note that `atomic_cell.take()` is equivalent to:
    ///
    /// ```ignore
    /// atomic_cell.replace(T::default())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.get(), 7);
    /// assert_eq!(a.take(), 7);
    /// assert_eq!(a.get(), 0);
    /// ```
    pub fn take(&self) -> T {
        self.replace(T::default())
    }
}

impl<T: Copy> AtomicCell<T> {
    /// Returns a copy of the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.get(), 7);
    /// ```
    pub fn get(&self) -> T {
        unsafe { atomic_load(self.value.get()) }
    }

    /// Updates the inner value using a function and returns the new value.
    ///
    /// Function `f` might have to be called multiple times if the inner value is concurrently
    /// changed by other threads.
    ///
    /// Note that `atomic_cell.update(f)` is equivalent to:
    ///
    /// ```ignore
    /// loop {
    ///     let current = atomic_cell.get();
    ///     let new = f(current);
    ///
    ///     if atomic_cell.compare_and_set(current, new) {
    ///         break new;
    ///     }
    /// }
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.update(|x| x.min(9)), 7);
    /// assert_eq!(a.update(|x| x.min(5)), 5);
    ///
    /// a.update(|x| x * 10);
    /// assert_eq!(a.get(), 50);
    /// ```
    pub fn update<F>(&self, mut f: F) -> T
    where
        F: FnMut(T) -> T,
    {
        let mut current = self.get();

        loop {
            let new = f(current);

            let previous = unsafe {
                atomic_compare_and_swap(self.value.get(), current, new)
            };

            if byte_eq(&previous, &current) {
                return new;
            }

            current = previous;
        }
    }
}

impl<T: Copy + Eq> AtomicCell<T> {
    /// If the current value equals `current`, stores `new` into the atomic cell.
    ///
    /// Returns `true` if the value was updated, and `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    ///
    /// assert_eq!(a.compare_and_set(1, 8), false);
    /// assert_eq!(a.get(), 7);
    ///
    /// assert_eq!(a.compare_and_set(7, 8), true);
    /// assert_eq!(a.get(), 8);
    /// ```
    pub fn compare_and_set(&self, mut current: T, new: T) -> bool {
        loop {
            let previous = unsafe {
                atomic_compare_and_swap(self.value.get(), current, new)
            };

            if byte_eq(&previous, &current) {
                return true;
            }

            if previous != current {
                return false;
            }

            // Since `byte_eq(&previous, &current)` is `false`, that means the compare-and-swap
            // operation failed and didn't store `new`. However, `previous == current`, which means
            // it technically should've succeeded.
            //
            // We cannot return neither `true` nor `false` here because the operation didn't
            // succeed nor fail, but simply encountered an inconsistent state. The only option left
            // is to retry with `previous` as the new `current`.
            current = previous;
        }
    }
}

macro_rules! impl_arithmetic {
    ($t:ty, $atomic:ty) => {
        impl AtomicCell<$t> {
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                let a = unsafe { &*(self.value.get() as *const $atomic) };
                a.fetch_add(val, Ordering::SeqCst).wrapping_add(val)
            }

            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                let a = unsafe { &*(self.value.get() as *const $atomic) };
                a.fetch_sub(val, Ordering::SeqCst).wrapping_sub(val)
            }
        }
    };
    ($t:ty) => {
        impl AtomicCell<$t> {
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                if mem::size_of::<$t>() == mem::size_of::<usize>() {
                    let a = unsafe { &*(self.value.get() as *const atomic::AtomicUsize) };
                    a.fetch_add(val as usize, Ordering::SeqCst).wrapping_add(val as usize) as $t
                } else {
                    let _lock = lock(self.value.get() as usize);
                    let value = unsafe { &mut *(self.value.get()) };
                    *value = value.wrapping_add(val);
                    *value
                }
            }

            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                if mem::size_of::<$t>() == mem::size_of::<usize>() {
                    let a = unsafe { &*(self.value.get() as *const atomic::AtomicUsize) };
                    a.fetch_sub(val as usize, Ordering::SeqCst).wrapping_sub(val as usize) as $t
                } else {
                    let _lock = lock(self.value.get() as usize);
                    let value = unsafe { &mut *(self.value.get()) };
                    *value = value.wrapping_sub(val);
                    *value
                }
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

struct AtomicUnit;

impl AtomicUnit {
    #[inline]
    fn load(&self, _order: Ordering) {}

    #[inline]
    fn store(&self, _val: (), _order: Ordering) {}

    #[inline]
    fn swap(&self, _val: (), _order: Ordering) {}

    #[inline]
    fn compare_and_swap(&self, _current: (), _new: (), _order: Ordering) {}
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
            atomic!(@check, $t, AtomicUnit, $a, $atomic_op);
            atomic!(@check, $t, atomic::AtomicBool, $a, $atomic_op);
            atomic!(@check, $t, atomic::AtomicUsize, $a, $atomic_op);

            #[cfg(feature = "nightly")]
            {
                #[cfg(target_has_atomic = "8")]
                atomic!(@check, $t, atomic::AtomicU8, $a, $atomic_op);
                #[cfg(target_has_atomic = "16")]
                atomic!(@check, $t, atomic::AtomicU16, $a, $atomic_op);
                #[cfg(target_has_atomic = "32")]
                atomic!(@check, $t, atomic::AtomicU32, $a, $atomic_op);
                #[cfg(target_has_atomic = "64")]
                atomic!(@check, $t, atomic::AtomicU64, $a, $atomic_op);
            }

            break $fallback_op
        }
    };
}

fn atomic_is_lock_free<T>() -> bool {
    atomic! { T, _a, true, false }
}

unsafe fn atomic_load<T>(dst: *mut T) -> T
where
    T: Copy,
{
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            mem::transmute_copy(&a.load(Ordering::SeqCst))
        },
        {
            let _lock = lock(dst as usize);
            ptr::read(dst)
        }
    }
}

unsafe fn atomic_store<T>(dst: *mut T, val: T) {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            a.store(mem::transmute_copy(&val), Ordering::SeqCst)
        },
        {
            let _lock = lock(dst as usize);
            ptr::write(dst, val)
        }
    }
}

unsafe fn atomic_swap<T>(dst: *mut T, val: T) -> T {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            mem::transmute_copy(&a.swap(mem::transmute_copy(&val), Ordering::SeqCst))
        },
        {
            let _lock = lock(dst as usize);
            ptr::replace(dst, val)
        }
    }
}

unsafe fn atomic_compare_and_swap<T>(dst: *mut T, current: T, new: T) -> T
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
                    Ordering::SeqCst,
                )
            )
        },
        {
            let _lock = lock(dst as usize);
            if byte_eq(&*dst, &current) {
                ptr::replace(dst, new)
            } else {
                ptr::read(dst)
            }
        }
    }
}
