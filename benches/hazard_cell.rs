#![feature(test)]

extern crate atomic;
extern crate crossbeam;
extern crate test;

use atomic::hazard_cell::HazardCell;

#[bench]
fn get(b: &mut test::Bencher) {
    let h = HazardCell::new(Box::new(777));
    b.iter(|| h.get());
}
