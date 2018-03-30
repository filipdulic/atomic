use std::cell::UnsafeCell;
use std::fmt;
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{self, AtomicBool, Ordering};

/// A thread-safe mutable memory location.
///
/// This type is equivalent to [`Cell`], except it can also be shared among multiple threads.
///
/// Operations on `AtomicCell`s use atomic instructions whenever possible, and synchronize using
/// global locks otherwise. You can call [`AtomicCell::<T>::is_lock_free()`] to check whether
/// atomic instructions or locks will be used.
///
/// [`Cell`]: https://doc.rust-lang.org/std/cell/struct.Cell.html
/// [`AtomicCell::<T>::is_lock_free()`]: struct.AtomicCell.html#method.is_lock_free
pub struct AtomicCell<T> {
    /// The inner value.
    ///
    /// If this value can be transmuted into a primitive atomic type, it will be treated as such.
    /// Otherwise, all potentially concurrent operations on this data will be protected by a global
    /// lock.
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
    /// If the compiler or the platform doesn't support the necessary atomic instructions,
    /// `AtomicCell<T>` will use global locks for every potentially concurrent atomic operation.
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
    /// // A wrapper struct around `isize`.
    /// struct Foo {
    ///     bar: isize,
    /// }
    /// // `AtomicCell<Foo>` will be internally represented as `AtomicIsize`.
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
    ($t:ty, $example:tt) => {
        impl AtomicCell<$t> {
            /// Increments the inner value by `val` and returns the new value.
            ///
            /// The addition wraps on overflow. Note that `atomic_cell.add(val)` is equivalent to:
            ///
            /// ```ignore
            /// atomic_cell.update(|x| x.wrapping_add(val))
            /// ```
            ///
            /// # Examples
            ///
            /// ```
            /// use atomic::AtomicCell;
            ///
            #[doc = $example]
            ///
            /// assert_eq!(a.add(3), 10);
            /// assert_eq!(a.get(), 10);
            /// ```
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                if can_transmute::<$t, atomic::AtomicUsize>() {
                    let a = unsafe { &*(self.value.get() as *const atomic::AtomicUsize) };
                    a.fetch_add(val as usize, Ordering::SeqCst).wrapping_add(val as usize) as $t
                } else {
                    let _lock = lock(self.value.get() as usize);
                    let value = unsafe { &mut *(self.value.get()) };
                    *value = value.wrapping_add(val);
                    *value
                }
            }

            /// Decrements the inner value by `val` and returns the new value.
            ///
            /// The subtraction wraps on overflow. Note that `atomic_cell.sub(val)` is equivalent
            /// to:
            ///
            /// ```ignore
            /// atomic_cell.update(|x| x.wrapping_sub(val))
            /// ```
            ///
            /// # Examples
            ///
            /// ```
            /// use atomic::AtomicCell;
            ///
            #[doc = $example]
            ///
            /// assert_eq!(a.sub(3), 4);
            /// assert_eq!(a.get(), 4);
            /// ```
            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                if can_transmute::<$t, atomic::AtomicUsize>() {
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
    ($t:ty, $atomic:ty, $example:tt) => {
        impl AtomicCell<$t> {
            /// Increments the inner value by `val` and returns the new value.
            ///
            /// The addition wraps on overflow. Note that `atomic_cell.add(val)` is equivalent to:
            ///
            /// ```ignore
            /// atomic_cell.update(|x| x.wrapping_add(val))
            /// ```
            ///
            /// # Examples
            ///
            /// ```
            /// use atomic::AtomicCell;
            ///
            #[doc = $example]
            ///
            /// assert_eq!(a.add(3), 10);
            /// assert_eq!(a.get(), 10);
            /// ```
            #[inline]
            pub fn add(&self, val: $t) -> $t {
                let a = unsafe { &*(self.value.get() as *const $atomic) };
                a.fetch_add(val, Ordering::SeqCst).wrapping_add(val)
            }

            /// Decrements the inner value by `val` and returns the new value.
            ///
            /// The subtraction wraps on overflow. Note that `atomic_cell.sub(val)` is equivalent
            /// to:
            ///
            /// ```ignore
            /// atomic_cell.update(|x| x.wrapping_sub(val))
            /// ```
            ///
            /// # Examples
            ///
            /// ```
            /// use atomic::AtomicCell;
            ///
            #[doc = $example]
            ///
            /// assert_eq!(a.sub(3), 4);
            /// assert_eq!(a.get(), 4);
            /// ```
            #[inline]
            pub fn sub(&self, val: $t) -> $t {
                let a = unsafe { &*(self.value.get() as *const $atomic) };
                a.fetch_sub(val, Ordering::SeqCst).wrapping_sub(val)
            }
        }
    };
}

cfg_if! {
    if #[cfg(feature = "nightly")] {
        impl_arithmetic!(u8, atomic::AtomicU8, "let a = AtomicCell::new(7u8);");
        impl_arithmetic!(i8, atomic::AtomicI8, "let a = AtomicCell::new(7i8);");
        impl_arithmetic!(u16, atomic::AtomicU16, "let a = AtomicCell::new(7u16);");
        impl_arithmetic!(i16, atomic::AtomicI16, "let a = AtomicCell::new(7i16);");
        impl_arithmetic!(u32, atomic::AtomicU32, "let a = AtomicCell::new(7u32);");
        impl_arithmetic!(i32, atomic::AtomicI32, "let a = AtomicCell::new(7i32);");
        impl_arithmetic!(u64, atomic::AtomicU64, "let a = AtomicCell::new(7u64);");
        impl_arithmetic!(i64, atomic::AtomicI64, "let a = AtomicCell::new(7i64);");
    } else {
        impl_arithmetic!(u8, "let a = AtomicCell::new(7u8);");
        impl_arithmetic!(i8, "let a = AtomicCell::new(7i8);");
        impl_arithmetic!(u16, "let a = AtomicCell::new(7u16);");
        impl_arithmetic!(i16, "let a = AtomicCell::new(7i16);");
        impl_arithmetic!(u32, "let a = AtomicCell::new(7u32);");
        impl_arithmetic!(i32, "let a = AtomicCell::new(7i32);");
        impl_arithmetic!(u64, "let a = AtomicCell::new(7u64);");
        impl_arithmetic!(i64, "let a = AtomicCell::new(7i64);");
    }
}

impl_arithmetic!(usize, atomic::AtomicUsize, "let a = AtomicCell::new(7usize);");
impl_arithmetic!(isize, atomic::AtomicIsize, "let a = AtomicCell::new(7isize);");

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

/// Returns `true` if the two values are equal byte-for-byte.
fn byte_eq<T>(a: &T, b: &T) -> bool {
    unsafe {
        let a = slice::from_raw_parts(a as *const _ as *const u8, mem::size_of::<T>());
        let b = slice::from_raw_parts(b as *const _ as *const u8, mem::size_of::<T>());
        a == b
    }
}

/// Returns `true` if values of type `A` can be transmuted into values of type `B`.
fn can_transmute<A, B>() -> bool {
    // Sizes must be equal, but alignment of `A` must be greater or equal than that of `B`.
    mem::size_of::<A>() == mem::size_of::<B>() && mem::align_of::<A>() >= mem::align_of::<B>()
}

/// Automatically releases a lock when dropped.
struct LockGuard {
    lock: &'static AtomicBool,
}

impl Drop for LockGuard {
    #[inline]
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

/// Acquires the lock for atomic data stored at the given address.
///
/// This function is used to protect atomic data which doesn't fit into any of the primitive atomic
/// types in `std::sync::atomic`. Operations on such atomics must therefore use a global lock.
///
/// However, there is not only one global lock but an array of many locks, and one of them is
/// picked based on the given address. Having many locks reduces contention and improves
/// scalability.
#[inline]
fn lock(addr: usize) -> LockGuard {
    // The number of locks is prime.
    const LEN: usize = 499;

    const A: AtomicBool = AtomicBool::new(false);
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

    // If the modulus is a constant number, the compiler will use crazy math to transform this into
    // a sequence of cheap arithmetic operations rather than using the slow modulo instruction.
    let lock = &LOCKS[addr % LEN];

    let mut step = 0usize;

    while lock.compare_and_swap(false, true, Ordering::Acquire) {
        if step < 5 {
            // Just try again.
        } else if step < 10 {
            atomic::spin_loop_hint();
        } else {
            #[cfg(not(feature = "use_std"))]
            atomic::spin_loop_hint();

            #[cfg(feature = "use_std")]
            ::std::thread::yield_now();
        }
        step = step.wrapping_add(1);
    }

    LockGuard { lock }
}

/// An atomic `()`.
///
/// All operations are noops.
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
    // If values of type `$t` can be transmuted into values of the primitive atomic type `$atomic`,
    // declares variable `$a` of type `$atomic` and executes `$atomic_op`, breaking out of the loop.
    (@check, $t:ty, $atomic:ty, $a:ident, $atomic_op:expr) => {
        if can_transmute::<$t, $atomic>() {
            let $a: &$atomic;
            break $atomic_op
        }
    };

    // If values of type `$t` can be transmuted into values of a primitive atomic type, declares
    // variable `$a` of that type and executes `$atomic_op`. Otherwise, just executes
    // `$fallback_op`.
    ($t:ty, $a:ident, $atomic_op:expr, $fallback_op:expr) => {
        loop {
            atomic!(@check, $t, AtomicUnit, $a, $atomic_op);
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

/// Returns `true` if operations on `AtomicCell<T>` are lock-free.
fn atomic_is_lock_free<T>() -> bool {
    atomic! { T, _a, true, false }
}

/// Atomically reads data from `src`.
///
/// This operation is sequentially consistent. If possible, an atomic instructions is used, and a
/// global lock otherwise.
unsafe fn atomic_load<T>(src: *mut T) -> T
where
    T: Copy,
{
    atomic! {
        T, a,
        {
            a = &*(src as *const _ as *const _);
            mem::transmute_copy(&a.load(Ordering::SeqCst))
        },
        {
            let _lock = lock(src as usize);
            ptr::read(src)
        }
    }
}

/// Atomically writes `val` to `dst`.
///
/// This operation is sequentially consistent. If possible, an atomic instructions is used, and a
/// global lock otherwise.
unsafe fn atomic_store<T>(dst: *mut T, val: T) {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            let res = a.store(mem::transmute_copy(&val), Ordering::SeqCst);
            mem::forget(val);
            res
        },
        {
            let _lock = lock(dst as usize);
            ptr::write(dst, val)
        }
    }
}

/// Atomically swaps data at `dst` with `val`.
///
/// This operation is sequentially consistent. If possible, an atomic instructions is used, and a
/// global lock otherwise.
unsafe fn atomic_swap<T>(dst: *mut T, val: T) -> T {
    atomic! {
        T, a,
        {
            a = &*(dst as *const _ as *const _);
            let res = mem::transmute_copy(&a.swap(mem::transmute_copy(&val), Ordering::SeqCst));
            mem::forget(val);
            res
        },
        {
            let _lock = lock(dst as usize);
            ptr::replace(dst, val)
        }
    }
}

/// Atomically compares data at `dst` to `current` and, if equal byte-for-byte, swaps data at `dst`
/// with `new`.
///
/// This operation is sequentially consistent. If possible, an atomic instructions is used, and a
/// global lock otherwise.
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
