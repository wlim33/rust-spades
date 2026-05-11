//! Poison-tolerant wrappers around stdlib `Mutex` / `RwLock`.
//!
//! Stdlib locks propagate panics via *poisoning*: if a holder panics while
//! the guard is live, every subsequent acquisition returns `Err(PoisonError)`.
//! Unwrapping that turns one panic in one task into a cascading outage as
//! every other task touching the lock panics in turn.
//!
//! The locks in this crate guard structures whose entries are independent —
//! `HashMap<Uuid, _>` of active timers / games / sessions, `Vec` queues of
//! seeks, presence tables. A panic mid-modification might leave an entry
//! half-formed, but the surrounding container remains traversable and the
//! other entries are untouched. Recovering from poison is therefore safe.
//!
//! The extension traits here add `lock_or_recover` / `read_or_recover` /
//! `write_or_recover` to the stdlib types so that call sites read
//! `self.active_timers.lock_or_recover()` rather than reaching for free
//! functions or inline `unwrap_or_else`.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait MutexExt<T: ?Sized> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> MutexExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub trait RwLockExt<T: ?Sized> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|p| p.into_inner())
    }
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn mutex_recovers_from_poison() {
        let m = Arc::new(Mutex::new(vec![1, 2, 3]));
        let m_clone = Arc::clone(&m);
        let _ = thread::spawn(move || {
            let mut g = m_clone.lock().unwrap();
            g.push(4);
            panic!("intentional");
        })
        .join();
        assert!(m.is_poisoned());
        let g = m.lock_or_recover();
        assert_eq!(*g, vec![1, 2, 3, 4]);
    }

    #[test]
    fn rwlock_read_recovers_from_poison() {
        let rw = Arc::new(RwLock::new(42_u32));
        let rw_clone = Arc::clone(&rw);
        let _ = thread::spawn(move || {
            let _g = rw_clone.write().unwrap();
            panic!("intentional");
        })
        .join();
        assert!(rw.is_poisoned());
        assert_eq!(*rw.read_or_recover(), 42);
    }

    #[test]
    fn rwlock_write_recovers_from_poison() {
        let rw = Arc::new(RwLock::new(0_u32));
        let rw_clone = Arc::clone(&rw);
        let _ = thread::spawn(move || {
            let mut g = rw_clone.write().unwrap();
            *g = 7;
            panic!("intentional");
        })
        .join();
        assert!(rw.is_poisoned());
        *rw.write_or_recover() = 9;
        assert_eq!(*rw.read_or_recover(), 9);
    }
}
