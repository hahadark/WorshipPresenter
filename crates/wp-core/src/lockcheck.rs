//! 코어 잠금: parking_lot::Mutex 를 싼 것. 같은 스레드가 잠금을 쥔 채 다시 잠그면(재진입)
//! parking_lot 은 영원히 멈춘다 — 예배 중이면 조작 화면이 얼어붙는다.
//! 그래서 스레드마다 "지금 쥐고 있는 잠금"을 적어 두고, 다시 잠그려 하면 멈추는 대신
//! 두 자리를 로그에 남기고 panic 한다. 조작 화면 프레임의 panic 은 app.rs 가 받아 그 프레임만 버리고
//! 계속 돌고, 서버 요청의 panic 은 그 요청만 실패한다 (멈춤보다 훨씬 낫다).
//! 또 잠금을 오래 쥔 자리(느린 일)를 로그로 알린다.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use web_time::Instant;

thread_local! {
    /// 이 스레드가 쥐고 있는 잠금들 (잠금 주소, 잠근 자리)
    static HELD: RefCell<Vec<(usize, &'static Location<'static>)>> = const { RefCell::new(Vec::new()) };
}

/// 이보다 오래 쥐면 로그에 남긴다 (ms). 조작 화면 한 프레임(16ms)보다 훨씬 길면 화면이 끊긴다.
const SLOW_HOLD_MS: u128 = 250;

/// 점검용: WP_DEBUG_LOCK_MS=30 처럼 주면 그 시간부터 남긴다 (짧게 자주 쥐는 자리 찾기)
fn slow_hold_ms() -> u128 {
    static V: once_cell::sync::Lazy<u128> =
        once_cell::sync::Lazy::new(|| std::env::var("WP_DEBUG_LOCK_MS").ok().and_then(|s| s.trim().parse().ok()).filter(|v| *v > 0).unwrap_or(SLOW_HOLD_MS));
    *V
}

pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
}

pub struct Guard<'a, T> {
    g: Option<parking_lot::MutexGuard<'a, T>>,
    addr: usize,
    at: &'static Location<'static>,
    since: Instant,
}

impl<T> Mutex<T> {
    pub fn new(v: T) -> Self {
        Mutex { inner: parking_lot::Mutex::new(v) }
    }

    fn addr(&self) -> usize {
        &self.inner as *const _ as usize
    }

    /// 잠근다. 같은 스레드가 이미 쥐고 있으면 멈추지 않고 panic (두 자리를 로그에).
    #[track_caller]
    pub fn lock(&self) -> Guard<'_, T> {
        let at = Location::caller();
        let addr = self.addr();
        if let Some(first) = held_at(addr) {
            log::error!("코어 잠금 재진입: {first} 에서 쥔 채 {at} 에서 다시 잠금 (멈춤 대신 이 작업만 취소)");
            panic!("코어 잠금 재진입: {first} 에서 쥔 채 {at} 에서 다시 잠금");
        }
        let g = self.inner.lock();
        HELD.with(|h| h.borrow_mut().push((addr, at)));
        Guard { g: Some(g), addr, at, since: Instant::now() }
    }

    /// 바로 잠글 수 있을 때만 (다른 스레드가 쥐고 있거나 이 스레드가 이미 쥐고 있으면 None)
    #[track_caller]
    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        let at = Location::caller();
        let addr = self.addr();
        if held_at(addr).is_some() {
            return None;
        }
        let g = self.inner.try_lock()?;
        HELD.with(|h| h.borrow_mut().push((addr, at)));
        Some(Guard { g: Some(g), addr, at, since: Instant::now() })
    }

    /// 이 스레드가 지금 이 잠금을 쥐고 있는지 (시험·점검용)
    pub fn held_by_me(&self) -> bool {
        held_at(self.addr()).is_some()
    }
}

fn held_at(addr: usize) -> Option<&'static Location<'static>> {
    HELD.try_with(|h| h.borrow().iter().find(|x| x.0 == addr).map(|x| x.1)).ok().flatten()
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.g.as_ref().expect("잠금")
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.g.as_mut().expect("잠금")
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.g.take();
        let addr = self.addr;
        let _ = HELD.try_with(|h| {
            let mut h = h.borrow_mut();
            if let Some(i) = h.iter().rposition(|x| x.0 == addr) {
                h.remove(i);
            }
        });
        let ms = self.since.elapsed().as_millis();
        if ms >= slow_hold_ms() && !std::thread::panicking() {
            note_slow(self.at, ms);
        }
    }
}

/// 오래 쥔 자리: 같은 자리는 처음과 그 뒤 가끔만 로그에 (로그가 넘치지 않게)
fn note_slow(at: &'static Location<'static>, ms: u128) {
    static SEEN: once_cell::sync::Lazy<parking_lot::Mutex<std::collections::HashMap<(&'static str, u32), u32>>> = once_cell::sync::Lazy::new(Default::default);
    let n = {
        let mut s = SEEN.lock();
        let n = s.entry((at.file(), at.line())).or_insert(0);
        *n += 1;
        *n
    };
    if n == 1 || n % 50 == 0 || std::env::var_os("WP_DEBUG_LOCK_MS").is_some() {
        log::warn!("코어 잠금을 오래 쥠: {at} {ms}ms ({n}번째) — 그동안 조작 화면·출력 요청이 기다립니다");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentry_panics_instead_of_hanging() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let m2 = m.clone();
        let r = std::thread::spawn(move || {
            let _a = m2.lock();
            let _b = m2.lock(); // 멈추면 이 시험이 끝나지 않는다
        })
        .join();
        assert!(r.is_err(), "재진입은 panic 이어야 한다");
        // panic 뒤에도 잠금이 풀려 있어 다른 스레드가 계속 쓸 수 있다 (parking_lot 은 오염되지 않음)
        assert_eq!(*m.lock(), 1);
        assert!(!m.held_by_me());
    }

    #[test]
    fn sequential_and_two_mutexes_ok() {
        let a = Mutex::new(1);
        let b = Mutex::new(2);
        {
            let ga = a.lock();
            let gb = b.lock(); // 다른 잠금은 함께 쥘 수 있다
            assert_eq!(*ga + *gb, 3);
            assert!(a.try_lock().is_none());
        }
        *a.lock() += 1;
        assert_eq!(*a.lock(), 2);
        assert!(a.try_lock().is_some());
    }
}
