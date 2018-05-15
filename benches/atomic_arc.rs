#![feature(test)]

extern crate atomic;
extern crate crossbeam;
extern crate test;

use atomic::hazard_cell::HazardCell;

#[bench]
fn get(b: &mut test::Bencher) {
    let h = HazardCell::new(Arc::new(777));
    b.iter(|| h.get());
}

use std::sync::*;
use std::cell::*;

#[bench]
fn replace(b: &mut test::Bencher) {
    let h = HazardCell::new(Arc::new(777));
    let a = Cell::new(Some(Arc::new(888)));
    b.iter(|| {
        let b = h.replace(a.take().unwrap());
        a.set(Some(b.clone()));
    });
}

#[bench]
fn load(b: &mut test::Bencher) {
    use std::sync::*;
    use std::sync::atomic::*;
    use std::sync::atomic::Ordering::*;
    use std::cell::*;

    let h = HazardCell::new(Arc::new(777));
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

    let h = HazardCell::new(Arc::new(777));
    let end = AtomicBool::new(false);
    crossbeam::scope(|s| {
        s.spawn(|| {
            let a = Cell::new(Some(Arc::new(888)));
            while !end.load(SeqCst) {
                for _ in 0..1000 {
                    let b = h.replace(a.take().unwrap());
                    a.set(Some(b.clone()));
                }
            }
        });
        s.spawn(|| {
            let a = Cell::new(Some(Arc::new(888)));
            b.iter(|| {
                let b = h.replace(a.take().unwrap());
                a.set(Some(b.clone()));
            });
            end.store(true, SeqCst);
        });
    });
}
