extern crate atomic;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

use atomic::atomic_cell::AtomicCell;

#[test]
fn is_lock_free() {
    struct UsizeWrap(usize);
    struct U8Wrap(bool);

    assert_eq!(AtomicCell::<usize>::is_lock_free(), true);
    assert_eq!(AtomicCell::<isize>::is_lock_free(), true);
    assert_eq!(AtomicCell::<UsizeWrap>::is_lock_free(), true);

    assert_eq!(AtomicCell::<u8>::is_lock_free(), cfg!(feature = "nightly"));
    assert_eq!(AtomicCell::<bool>::is_lock_free(), cfg!(feature = "nightly"));
    assert_eq!(AtomicCell::<U8Wrap>::is_lock_free(), cfg!(feature = "nightly"));
}

#[test]
fn drops_unit() {
    static CNT: AtomicUsize = AtomicUsize::new(0);
    CNT.store(0, SeqCst);

    #[derive(Debug, PartialEq, Eq)]
    struct Foo();

    impl Foo {
        fn new() -> Foo {
            CNT.fetch_add(1, SeqCst);
            Foo()
        }
    }

    impl Drop for Foo {
        fn drop(&mut self) {
            CNT.fetch_sub(1, SeqCst);
        }
    }

    impl Default for Foo {
        fn default() -> Foo {
            Foo::new()
        }
    }

    let a = AtomicCell::new(Foo::new());

    assert_eq!(a.replace(Foo::new()), Foo::new());
    assert_eq!(CNT.load(SeqCst), 1);

    a.set(Foo::new());
    assert_eq!(CNT.load(SeqCst), 1);

    assert_eq!(a.take(), Foo::new());
    assert_eq!(CNT.load(SeqCst), 1);

    drop(a);
    assert_eq!(CNT.load(SeqCst), 0);
}

#[test]
fn drops_u8() {
    static CNT: AtomicUsize = AtomicUsize::new(0);
    CNT.store(0, SeqCst);

    #[derive(Debug, PartialEq, Eq)]
    struct Foo(u8);

    impl Foo {
        fn new(val: u8) -> Foo {
            CNT.fetch_add(1, SeqCst);
            Foo(val)
        }
    }

    impl Drop for Foo {
        fn drop(&mut self) {
            CNT.fetch_sub(1, SeqCst);
        }
    }

    impl Default for Foo {
        fn default() -> Foo {
            Foo::new(0)
        }
    }

    let a = AtomicCell::new(Foo::new(5));

    assert_eq!(a.replace(Foo::new(6)), Foo::new(5));
    assert_eq!(a.replace(Foo::new(1)), Foo::new(6));
    assert_eq!(CNT.load(SeqCst), 1);

    a.set(Foo::new(2));
    assert_eq!(CNT.load(SeqCst), 1);

    assert_eq!(a.take(), Foo::new(2));
    assert_eq!(CNT.load(SeqCst), 1);

    assert_eq!(a.take(), Foo::new(0));
    assert_eq!(CNT.load(SeqCst), 1);

    drop(a);
    assert_eq!(CNT.load(SeqCst), 0);
}

#[test]
fn drops_usize() {
    static CNT: AtomicUsize = AtomicUsize::new(0);
    CNT.store(0, SeqCst);

    #[derive(Debug, PartialEq, Eq)]
    struct Foo(usize);

    impl Foo {
        fn new(val: usize) -> Foo {
            CNT.fetch_add(1, SeqCst);
            Foo(val)
        }
    }

    impl Drop for Foo {
        fn drop(&mut self) {
            CNT.fetch_sub(1, SeqCst);
        }
    }

    impl Default for Foo {
        fn default() -> Foo {
            Foo::new(0)
        }
    }

    let a = AtomicCell::new(Foo::new(5));

    assert_eq!(a.replace(Foo::new(6)), Foo::new(5));
    assert_eq!(a.replace(Foo::new(1)), Foo::new(6));
    assert_eq!(CNT.load(SeqCst), 1);

    a.set(Foo::new(2));
    assert_eq!(CNT.load(SeqCst), 1);

    assert_eq!(a.take(), Foo::new(2));
    assert_eq!(CNT.load(SeqCst), 1);

    assert_eq!(a.take(), Foo::new(0));
    assert_eq!(CNT.load(SeqCst), 1);

    drop(a);
    assert_eq!(CNT.load(SeqCst), 0);
}

#[test]
fn modular_u8() {
    #[derive(Clone, Copy, Eq, Debug, Default)]
    struct Foo(u8);

    impl PartialEq for Foo {
        fn eq(&self, other: &Foo) -> bool {
            self.0 % 5 == other.0 % 5
        }
    }

    let a = AtomicCell::new(Foo(1));

    assert_eq!(a.get(), Foo(1));
    assert_eq!(a.replace(Foo(2)), Foo(11));
    assert_eq!(a.get(), Foo(52));

    assert_eq!(a.update(|_| Foo(3)), Foo(33));
    assert_ne!(a.update(|_| Foo(3)).0, 33);
    assert_eq!(a.update(|_| Foo(44)).0, 44);

    a.set(Foo(0));
    let mut x = 0;
    let new = a.update(|old| {
        if x < 20 {
            x += 1;
            a.set(Foo(x));
        }
        Foo(0)
    });
    assert_eq!(x, 20);
    assert_eq!(new.0, 0);

    a.set(Foo(0));
    assert_eq!(a.compare_and_set(Foo(0), Foo(5)), true);
    assert_eq!(a.get().0, 5);
    assert_eq!(a.compare_and_set(Foo(10), Foo(15)), true);
    assert_eq!(a.get().0, 15);
}

#[test]
fn modular_usize() {
    #[derive(Clone, Copy, Eq, Debug, Default)]
    struct Foo(usize);

    impl PartialEq for Foo {
        fn eq(&self, other: &Foo) -> bool {
            self.0 % 5 == other.0 % 5
        }
    }

    let a = AtomicCell::new(Foo(1));

    assert_eq!(a.get(), Foo(1));
    assert_eq!(a.replace(Foo(2)), Foo(11));
    assert_eq!(a.get(), Foo(52));

    assert_eq!(a.update(|_| Foo(3)), Foo(33));
    assert_ne!(a.update(|_| Foo(3)).0, 33);
    assert_eq!(a.update(|_| Foo(44)).0, 44);

    a.set(Foo(0));
    let mut x = 0;
    let new = a.update(|old| {
        if x < 20 {
            x += 1;
            a.set(Foo(x));
        }
        Foo(0)
    });
    assert_eq!(x, 20);
    assert_eq!(new.0, 0);

    a.set(Foo(0));
    assert_eq!(a.compare_and_set(Foo(0), Foo(5)), true);
    assert_eq!(a.get().0, 5);
    assert_eq!(a.compare_and_set(Foo(10), Foo(15)), true);
    assert_eq!(a.get().0, 15);
}
