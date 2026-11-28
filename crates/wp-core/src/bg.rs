//! 백그라운드 일 한 겹 (웹판 대비). 설치판: `std::thread::spawn` 그대로.
//! 웹판(wasm32): 스레드가 없고 `std::thread::spawn` 은 panic(조작 화면 전체가 멈춤) → 돌리지 않고 '설치판에서 해 주세요' 를 알린다.
//! (답을 기다리는 AI · PPT 그리기처럼 웹에서도 해야 하는 짧은 일은 `hymns::run_bg` 가 같은 스레드의 다음 차례로 돌린다)
//! 다른 가지를 합친 뒤 새로 생긴 `std::thread::spawn(` 은 `python tools/web/rethread.py` 로 다시 바꾼다.

pub fn spawn<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::spawn(f);
    }
    #[cfg(target_arch = "wasm32")]
    {
        drop(f);
        unsupported();
    }
}

#[cfg(target_arch = "wasm32")]
fn unsupported() {
    log::warn!("웹판에서 돌릴 수 없는 백그라운드 일을 건너뜀");
    if let Some(sh) = crate::hymns::shared() {
        // 지금 잡혀 있으면(부른 쪽이 코어를 쥔 채) 다음 차례에 알린다
        crate::web::defer(0, move || {
            if let Some(mut c) = sh.try_lock() {
                c.notify(crate::i18n::tr("이 기능은 웹판에서 아직 쓸 수 없습니다. 설치판에서 해 주세요."), "error");
            }
        });
    }
}
