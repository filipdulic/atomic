extern crate atomic;
extern crate crossbeam;
extern crate parking_lot;

use std::sync::Arc;

use atomic::hazard_cell::HazardCell;
use parking_lot::Mutex;

struct Node<T> {
    value: Mutex<Option<T>>,
    next: HazardCell<Option<Arc<Node<T>>>>,
}

impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        // TODO: Drop the chain of nodes iteratively rather than recursively.
    }
}

struct Stack<T> {
    head: HazardCell<Option<Arc<Node<T>>>>,
}

impl<T> Stack<T> {
    fn new() -> Stack<T> {
        Stack {
            head: HazardCell::new(None),
        }
    }

    fn push(&self, value: T) {
        let mut new = Arc::new(Node {
            value: Mutex::new(Some(value)),
            next: HazardCell::new(None),
        });

        loop {
            let head = self.head.get();
            new.next.set(head.clone());

            match self.head.compare_and_set(&head, Some(new)) {
                Ok(()) => break,
                Err(n) => new = n.unwrap(),
            }
        }
    }

    fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.get();

            match *head {
                None => return None,
                Some(ref h) => {
                    if self.head.compare_and_set(&head, h.next.get().clone()).is_ok() {
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
    // let s = crossbeam::sync::TreiberStack::new();

    crossbeam::scope(|scope| {
        for _ in 0..T {
            scope.spawn(|| {
                for i in 0 .. N / T {
                    s.push(i);
                }
                for _ in 0 .. N / T {
                    s.pop().unwrap();
                }
            });
        }
    });
}
