# 워십프리젠터 (Rust 판)

교회 예배 순서(큐시트) 통합 송출 프로그램 [워십프리젠터](https://github.com/hahadark/WorshipPresenter) 를 Rust 로 다시 만드는 중입니다.

- 조작 화면: Rust + egui (Electron 없이 가볍게)
- 예배당 모니터: WebView2 출력 창 · OBS: 브라우저 소스 주소는 예전과 같음 (`http://127.0.0.1:37528/outputs/main`, `/outputs/overlay`)
- 자료: 처음 실행할 때 기존 워십프리젠터 자료(성경·큐시트·설정)를 복사해 옵니다. 원본은 바꾸지 않습니다.

현재는 로컬 개발·통합 검증 단계입니다. 성경·찬양·찬송가/PPT 송출 외에 템플릿, 슬라이드 편집, 라이브러리, 무대 화면, 리모컨, 연동, 백업, 번역본·주석 가져오기 등을 옮기고 있습니다. 기능이 전부 완성되었다는 뜻은 아닙니다. 구현 대조와 검증 범위는 `PARITY.md`, 작업 규칙은 `CLAUDE.md`를 보세요.

**Electron 판의 모든 기능을 구현할 때까지 릴리스·설치 파일·GitHub 업로드는 중지합니다.**

## 개발·검증

```powershell
cargo test --workspace
cargo build
.\target\debug\wp-app.exe --data .\tmp\test-data --search "요3:16" --send --shot .\tmp\test.png
```

점검은 별도 `--data` 폴더에서 합니다. 처음 실행 시 기존 자료가 복사될 수 있으므로 이 폴더를 공개 저장소에 넣지 마세요. 사용자 원본 자료와 이미 실행 중인 앱은 변경하거나 종료하지 않습니다.

라이선스: GPL-3.0-or-later
