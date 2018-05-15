extern crate atomic;
extern crate crossbeam;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use atomic::AtomicArc;

static DROP_PER_THREAD: usize = 1000000;
static N_THREADS: usize = 8;

static DROP_CNT: AtomicUsize = AtomicUsize::new(0);

struct Foo(AtomicUsize);

impl Drop for Foo {
    fn drop(&mut self) {
        DROP_CNT.fetch_add(1, Ordering::SeqCst);
    }
}

fn work(cell: &AtomicArc<Foo>) {
    for _ in 0..DROP_PER_THREAD {
        let new_data = Arc::new(Foo(AtomicUsize::new(0)));
        cell.replace(new_data);
    }
}

#[test]
fn test_replace() {

    let element = AtomicArc::new(Arc::new(Foo(AtomicUsize::new(0))));

    crossbeam::scope(|s| {
        for _ in 0..N_THREADS {
            s.spawn(|| work(&element));
        }
    });

    assert_eq!(DROP_CNT.load(Ordering::Relaxed), N_THREADS * DROP_PER_THREAD);
}

