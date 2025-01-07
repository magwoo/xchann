use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Sender<T> {
    write_index: Arc<UnsafeCell<usize>>,
    read_index: Arc<AtomicUsize>,
    buffer: Arc<Vec<UnsafeCell<Option<T>>>>,
}

#[derive(Clone)]
pub struct Reciver<T> {
    read_index: Arc<AtomicUsize>,
    write_index: Arc<UnsafeCell<usize>>,
    buffer: Arc<Vec<UnsafeCell<Option<T>>>>,
}

unsafe impl<T> Send for Reciver<T> {}
unsafe impl<T> Send for Sender<T> {}

pub fn bounded<T>(size: usize) -> (Sender<T>, Reciver<T>) {
    let buffer = Arc::new((0..size).map(|_| UnsafeCell::new(None)).collect::<Vec<_>>());

    #[allow(clippy::arc_with_non_send_sync)]
    let write_index = Arc::new(UnsafeCell::new(0));
    let read_index = Arc::new(AtomicUsize::new(0));

    let sender = Sender {
        write_index: write_index.clone(),
        read_index: read_index.clone(),
        buffer: buffer.clone(),
    };

    let reciver = Reciver {
        read_index,
        write_index,
        buffer,
    };

    (sender, reciver)
}

impl<T> Sender<T> {
    pub fn try_send(&self, value: T) -> Result<(), T> {
        let current_write = unsafe { *self.write_index.get() };
        let next_write = (current_write + 1) % self.buffer.len();

        if next_write == self.read_index.load(Ordering::Acquire) {
            return Err(value);
        }

        let slot = &self.buffer[current_write];

        unsafe {
            *slot.get() = Some(value);
            *self.write_index.get() = next_write;
        }

        Ok(())
    }

    pub fn send(&self, value: T) {
        let mut value = Some(value);

        loop {
            match self.try_send(value.take().unwrap()) {
                Ok(()) => break,
                Err(e) => value = Some(e),
            }

            std::thread::yield_now();
        }
    }
}

impl<T> Reciver<T> {
    pub fn try_recv(&self) -> Option<T> {
        let read =
            self.read_index
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current_read| {
                    let current_write = unsafe { *self.write_index.get() };
                    if current_read != current_write {
                        Some((current_read + 1) % self.buffer.len())
                    } else {
                        None
                    }
                });

        match read {
            Ok(prev_read) => {
                let value = &self.buffer[prev_read];
                unsafe { (*value.get()).take() }
            }
            Err(_) => None,
        }
    }

    pub fn recv(&self) -> T {
        loop {
            if let Some(value) = self.try_recv() {
                return value;
            }

            std::thread::yield_now();
        }
    }
}
