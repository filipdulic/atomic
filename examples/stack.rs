extern crate atomic;

use std::sync::{Arc, Mutex};

use atomic::hazard_cell::HazardCell;

struct Node<T> {
    value: Mutex<Option<T>>,
    next: HazardCell<Option<Arc<Node<T>>>>,
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

            match head.as_ref() {
                None => return None,
                Some(h) => {
                    let next = h.next.get().clone();

                    if self.head.compare_and_set(&head, next).is_ok() {
                        return h.value.lock().unwrap().take();
                    }
                }
            }
        }
    }
}

fn main() {
    let s = Stack::<i32>::new();
    s.push(10);
    s.push(20);
    s.push(30);
    println!("{:?}", s.pop());
    s.push(40);
    println!("{:?}", s.pop());
    println!("{:?}", s.pop());
    println!("{:?}", s.pop());
    println!("{:?}", s.pop());
}
