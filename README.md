# 워십프리젠터 2.x (Rust 판)

교회 예배 순서(큐시트) 통합 송출 프로그램 [워십프리젠터](https://github.com/hahadark/WorshipPresenter)의 2.x 소스입니다. 2.0.0부터 Windows 정식판이고, 지금 공개판은 **2.0.2**입니다.

- 조작 화면: Rust + egui (Electron 없이)
- 예배당 모니터: WebView2 출력 창(`wp-output.exe`). 첫 화면을 다 그린 뒤 한 번에 보입니다.
- OBS: 브라우저 소스 주소는 1.x와 같습니다 — `http://127.0.0.1:37528/outputs/main`(예배당 화면 그대로), `/outputs/overlay`(투명 자막), `/outputs/stage`(무대 화면). 포트가 쓰이면 다음 번호를 씁니다.
- 자료: `%APPDATA%\WorshipPresenter-rs\data`. 처음 켤 때 1.x 자료(`%APPDATA%\WorshipCue\data`)를 복사해 오고, 1.x 원본은 바꾸지 않습니다.

사용법은 [README(main 가지)](https://github.com/hahadark/WorshipPresenter#readme)와 [사용자 설명서](https://github.com/hahadark/WorshipPresenter/blob/main/docs/%EC%82%AC%EC%9A%A9%EC%9E%90%EC%84%A4%EB%AA%85%EC%84%9C.md)를 보세요. 1.x(Electron) 기능과의 대조·검증 범위는 `PARITY.md`, 작업 규칙은 `CLAUDE.md`에 있습니다. 맥과 영어판 2.x는 아직 없습니다(1.14.1 사용).

## 설치·업데이트

- [릴리스](https://github.com/hahadark/WorshipPresenter/releases/latest)의 `WorshipPresenter-X.Y.Z-Setup.exe`(설치판) 또는 `…-Portable.zip`.
- 1.x 설치판은 끌 때 자동 업데이트(`latest.yml`)로 같은 자리에 2.x가 설치됩니다.
- 2.x끼리는 **도움말 → 새 버전 확인 → [검증하여 다운로드] → [지금 설치 요청]**.

## 개발·검증

```powershell
cargo test --workspace
cargo build
.\target\debug\wp-app.exe --data .\tmp\test-data --search "요3:16" --send --shot .\tmp\test.png
```

- 점검은 별도 `--data` 폴더에서 합니다. 기존 자료 복사를 막으려면 `WP_LEGACY_DATA`를 빈 폴더로 지정하세요. 이 폴더를 저장소에 넣지 마세요. 사용자 원본 자료와 이미 실행 중인 앱은 변경하거나 종료하지 않습니다.
- 릴리스 전에는 `tools\parity\run.ps1`과 `tools\parity\verify.js`로 조작 화면 그림과 실제 웹 출력을 나란히 비교합니다(`docs/slide-parity.md`).
- 릴리스 파일: `tools\release.ps1 -Version X.Y.Z` → `dist\`에 Setup.exe · Portable.zip · latest.yml. 순서는 `CLAUDE.md` 참고.

라이선스: GPL-3.0-or-later