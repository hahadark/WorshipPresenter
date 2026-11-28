//! '프로그램에 하나' 인 화면 상태 (창이 열렸는지 · 그 창의 입력값).
//!
//! 프로그램에서는 전역 Mutex 하나 — 예전의 `static ST: Lazy<Mutex<St>>` 와 똑같이 `ST.lock()` 으로 쓴다.
//!
//! 시험에서는 시험마다 따로 둔다. 화면 전체 합성 시험(App::ui_body)은 여러 스레드에서 동시에 도는데,
//! 창 상태가 프로세스 전체 값이면 한 시험이 연 창(무대 화면 · 주보 · 도구 …)이 같은 때 도는 다른 시험의 화면에도 떠서
//! 누를 자리를 가린다 → 누름 · 두 번 누름이 가끔 안 먹는다 (2.1.1 맥 CI 의 slideedit_focus_tests,
//! dbl_preview_chip_moves_and_sends 가 그렇게 흔들렸다). 그래서 시험 빌드에서는 '구역' 마다 상태를 따로 만든다:
//! - 구역은 스레드마다 하나, `App::for_test` 가 부를 때마다 새로 (`new_scope`) → 시험 하나 = 구역 하나
//! - 화면 코드가 띄운 일꾼 스레드가 같은 상태에 결과를 써야 하면 `uistate::spawn` 으로 띄운다 (구역을 물려받음)

use parking_lot::{Mutex, MutexGuard};

pub struct Global<T: 'static> {
    init: fn() -> T,
    #[cfg(not(test))]
    cell: once_cell::sync::OnceCell<Mutex<T>>,
}

impl<T: Send + 'static> Global<T> {
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            init,
            #[cfg(not(test))]
            cell: once_cell::sync::OnceCell::new(),
        }
    }

    #[cfg(not(test))]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.cell.get_or_init(|| Mutex::new((self.init)())).lock()
    }

    /// 시험: 지금 구역의 상태 (없으면 처음 값으로 만든다)
    #[cfg(test)]
    pub fn lock(&'static self) -> MutexGuard<'static, T> {
        scoped::get(self as *const Self as usize, self.init).lock()
    }
}

/// 화면 상태에 결과를 쓰는 일꾼 스레드 띄우기. 프로그램에서는 `std::thread::spawn` 그대로
#[cfg(not(test))]
pub fn spawn<F: FnOnce() + Send + 'static>(f: F) -> std::thread::JoinHandle<()> {
    std::thread::spawn(f)
}

/// 시험: 띄운 쪽의 구역을 물려준다 (일꾼이 쓴 결과를 그 시험의 화면이 읽게)
#[cfg(test)]
pub fn spawn<F: FnOnce() + Send + 'static>(f: F) -> std::thread::JoinHandle<()> {
    let scope = scoped::current();
    std::thread::spawn(move || {
        scoped::enter(scope);
        f()
    })
}

/// 시험: 이 스레드에서 지금부터 새 구역 (창이 모두 닫힌 처음 상태). `App::for_test` 가 부른다
#[cfg(test)]
pub fn new_scope() {
    scoped::enter(scoped::fresh());
}

#[cfg(test)]
mod scoped {
    use parking_lot::Mutex;
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    thread_local! {
        static SCOPE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    type Slot = &'static (dyn Any + Send + Sync);
    static MAP: Mutex<Option<HashMap<(u64, usize), Slot>>> = Mutex::new(None);

    pub fn fresh() -> u64 {
        NEXT.fetch_add(1, Ordering::Relaxed)
    }
    pub fn enter(id: u64) {
        SCOPE.with(|s| s.set(id));
    }
    pub fn current() -> u64 {
        SCOPE.with(|s| {
            if s.get() == 0 {
                s.set(fresh());
            }
            s.get()
        })
    }
    /// (구역, 전역 값의 주소) → 그 구역의 상태. 시험 프로세스가 끝날 때까지 두므로 일부러 흘린다 (작은 구조체 몇백 개)
    pub fn get<T: Send + 'static>(addr: usize, init: fn() -> T) -> &'static Mutex<T> {
        let key = (current(), addr);
        let mut map = MAP.lock();
        let slot: Slot = *map.get_or_insert_with(HashMap::new).entry(key).or_insert_with(|| {
            let leaked: &'static Mutex<T> = Box::leak(Box::new(Mutex::new(init())));
            leaked
        });
        drop(map);
        slot.downcast_ref::<Mutex<T>>().expect("같은 주소의 전역 값은 같은 형")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static COUNT: Global<u32> = Global::new(|| 7);

    #[test]
    fn each_test_scope_starts_from_the_initial_value() {
        new_scope();
        assert_eq!(*COUNT.lock(), 7);
        *COUNT.lock() += 1;
        assert_eq!(*COUNT.lock(), 8, "같은 구역에서는 같은 값");
        new_scope();
        assert_eq!(*COUNT.lock(), 7, "새 구역은 처음 값 (앞 시험이 연 창이 남지 않는다)");
    }

    #[test]
    fn other_threads_do_not_see_each_others_state_but_spawned_workers_do() {
        new_scope();
        *COUNT.lock() = 100;
        // 다른 시험 스레드: 따로
        let other = std::thread::spawn(|| *COUNT.lock()).join().unwrap();
        assert_eq!(other, 7, "병렬로 도는 다른 시험은 이 시험의 상태를 보지 않는다");
        // 화면 코드가 띄운 일꾼: 같은 상태
        spawn(|| *COUNT.lock() += 1).join().unwrap();
        assert_eq!(*COUNT.lock(), 101, "일꾼이 쓴 결과는 띄운 시험의 화면이 읽는다");
    }
}
