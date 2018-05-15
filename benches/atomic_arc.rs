#![feature(test)]

extern crate atomic;
extern crate crossbeam;
extern crate test;

use std::cell::Cell;
use std::sync::Arc;

use atomic::AtomicArc;

#[bench]
fn get(b: &mut test::Bencher) {
    let h = AtomicArc::new(Arc::new(777));
    b.iter(|| h.get());
}

#[bench]
fn replace(b: &mut test::Bencher) {
    let h = AtomicArc::new(Arc::new(777));
    let a = Cell::new(Some(Arc::new(888)));
    b.iter(|| {
        let b = h.replace(a.take().unwrap());
        a.set(b.clone_inner());
    });
}

#[bench]
fn load(b: &mut test::Bencher) {
    use std::sync::*;
    use std::sync::atomic::*;
    use std::sync::atomic::Ordering::*;
    use std::cell::*;

    let h = AtomicArc::new(Arc::new(777));
    let end = AtomicBool::new(false);
    crossbeam::scope(|s| {
        s.spawn(|| {
            while !end.load(SeqCst) {
                for _ in 0..1000 {
                    h.get();
                }
            }
        });
        s.spawn(|| {
            b.iter(|| h.get());
            end.store(true, SeqCst);
        });
    });
}

#[bench]
fn swap(b: &mut test::Bencher) {
    use std::sync::*;
    use std::sync::atomic::*;
    use std::sync::atomic::Ordering::*;
    use std::cell::*;

    let h = AtomicArc::new(Arc::new(777));
    let end = AtomicBool::new(false);
    crossbeam::scope(|s| {
        s.spawn(|| {
            let a = Cell::new(Some(Arc::new(888)));
            while !end.load(SeqCst) {
                for _ in 0..1000 {
                    let b = h.replace(a.take().unwrap());
                    a.set(b.clone_inner());
                }
            }
        });
        s.spawn(|| {
            let a = Cell::new(Some(Arc::new(888)));
            b.iter(|| {
                let b = h.replace(a.take().unwrap());
                a.set(b.clone_inner());
            });
            end.store(true, SeqCst);
        });
    });
}
