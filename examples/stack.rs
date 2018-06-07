extern crate atomic;
extern crate crossbeam;
extern crate parking_lot;

use std::sync::Arc;

use atomic::AtomicArc;
use parking_lot::Mutex;

struct Node<T> {
    value: Mutex<Option<T>>,
    next: AtomicArc<Node<T>>,
}

impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        let mut next = self.next.replace(None);
        loop {
            next = match next.as_ref() {
                None => break,
                Some(n) => n.next.replace(None),
            };
        }
    }
}

struct Stack<T> {
    head: AtomicArc<Node<T>>,
}

impl<T> Stack<T> {
    fn new() -> Stack<T> {
        Stack {
            head: AtomicArc::new(None),
        }
    }

    fn push(&self, value: T) {
        let mut new = Arc::new(Node {
            value: Mutex::new(Some(value)),
            next: AtomicArc::new(None),
        });

        loop {
            let head = self.head.get();
            new.next.set(&head);

            match self.head.compare_and_set(&head, new) {
                Ok(()) => break,
                Err(n) => new = n.unwrap(),
            }
        }
    }

    fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.get();

            match head.as_ref() {
                None => return None,
                Some(h) => {
                    if self.head.compare_and_set(&head, h.next.get()).is_ok() {
                        h.next.set(None);
                        return h.value.lock().take();
                    }
                }
            }
        }
    }
}

fn main() {
    const N: usize = 1_000_000;
    const T: usize = 8;

    let s = Stack::new();
    crossbeam::scope(|scope| {
        for _ in 0..T {
            scope.spawn(|| {
                for i in 0 .. N / T {
                    s.push(i);
                }
                for i in 0 .. N / T {
                    while s.pop().is_none() {
                        std::thread::yield_now();
                    }
                }
            });
        }
    });
}
