#![feature(test)]

extern crate atomic;
extern crate test;

use atomic::AtomicCell;

#[bench]
fn get_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    let mut sum = 0;
    b.iter(|| sum += a.get());
    test::black_box(sum);
}

#[bench]
fn set_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    b.iter(|| a.set(1));
}

#[bench]
fn take_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    b.iter(|| a.take());
}

#[bench]
fn add_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    b.iter(|| a.add(1));
}

#[bench]
fn update_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    b.iter(|| a.update(|x| x.wrapping_add(1)));
}

#[bench]
fn compare_and_set_u8(b: &mut test::Bencher) {
    let a = AtomicCell::new(0u8);
    let mut i = 0;
    b.iter(|| {
        a.compare_and_set(i, i.wrapping_add(1));
        i = i.wrapping_add(1);
    });
}

#[bench]
fn get_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    let mut sum = 0;
    b.iter(|| sum += a.get());
    test::black_box(sum);
}

#[bench]
fn set_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    b.iter(|| a.set(1));
}

#[bench]
fn take_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    b.iter(|| a.take());
}

#[bench]
fn add_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    b.iter(|| a.add(1));
}

#[bench]
fn update_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    b.iter(|| a.update(|x| x.wrapping_add(1)));
}

#[bench]
fn compare_and_set_usize(b: &mut test::Bencher) {
    let a = AtomicCell::new(0usize);
    let mut i = 0;
    b.iter(|| {
        a.compare_and_set(i, i.wrapping_add(1));
        i = i.wrapping_add(1);
    });
}
