# 워십프리젠터 (Rust 판)

교회 예배 순서(큐시트) 통합 송출 프로그램 [워십프리젠터](https://github.com/hahadark/WorshipPresenter) 를 Rust 로 다시 만드는 중입니다.

- 조작 화면: Rust + egui (Electron 없이 가볍게)
- 예배당 모니터: WebView2 출력 창 · OBS: 브라우저 소스 주소는 예전과 같음 (`http://127.0.0.1:37528/outputs/main`, `/outputs/overlay`)
- 자료: 처음 실행할 때 기존 워십프리젠터 자료(성경·큐시트·설정)를 복사해 옵니다. 원본은 바꾸지 않습니다.

지금은 1단계 미리보기 판입니다. 성경 찾기·송출, 큐시트(성경·찬양·공지·제목·빈 화면·그림) 송출이 됩니다.
찬송가·PPT 순서와 그 밖의 기능은 차례로 옮기는 중입니다. 자세한 상태는 `CLAUDE.md` 를 보세요.

## 빌드

```
cargo build --release
target\release\wp-app.exe
```

라이선스: GPL-3.0-or-later
