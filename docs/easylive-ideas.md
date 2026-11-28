# 쉬운방송에서 가져온 아이디어 (2026-11-26)

쉬운방송(목회ai연구소, 0.9.3)을 분석해 워십프리젠터에 없는 것 가운데 쓸 만한 생각만 가져와 새로 짰다. 쉬운방송에는 라이선스가 없으므로 코드는 한 줄도 옮기지 않았다.

## 1. 휴대폰 리모컨 → 무대 화면 말 보내기
- 원래 있던 기능(메시지 · 미리 적은 말 · 타이머). 리모컨에 '지금 무대 화면에 떠 있는 말 · 타이머 남은 시간'만 더 보이게 했다.

## 2. 이름 자막 (names_ui.rs · overlays.rs · render.js)
- 송출 → 이름 자막 (설교자 · 기도자). 명단 `settings.people [{name, role}]`, 하나뿐인 오버레이 `id nametag, type name { text 이름, sub 직분, style bar|box|plain, accent, hideAt }`.
- 기본 보이는 곳은 OBS 만(`outputs ["overlay"]`), 예배당도 켤 수 있다. `nameTagSeconds` 초가 지나면 `flow::tick` 이 내린다.
- 휴대폰 리모컨: `act {action:"name", index}` (index < 0 이면 내리기), `state.names {people, on}`.

## 3. 방화벽 확인 · 열기 (firewall.rs · firewall_ui.rs)
- 확인: PowerShell `Get-NetFirewallApplicationFilter -Program <이 exe>` 규칙 + `Get-NetConnectionProfile` + `Get-NetFirewallProfile` (읽기만, 1~3초, 다른 스레드).
- 열기: UAC 한 번. 이 exe 의 받는 쪽 '막기' 규칙을 지우고 `WorshipPresenter (휴대폰 · 내부망)` 허용 규칙 하나 (`-Profile Any -RemoteAddress LocalSubnet`).
- 연결 점검 · 휴대폰 리모컨 · 내부망 출력 · 휴대폰 카메라 창에 같은 줄. 실제 UAC 경로는 사용자 화면에 창이 뜨므로 자동 점검하지 않았다.

## 4. 휴대폰을 카메라로 (phonecam.rs · web/phone/cam.html · render.js)
- 연결 → 휴대폰 카메라. 설정 `phoneCam { enabled, port 37560, key }`. QR = `https://<내부망 IP>:37560/cam?k=<열쇠>`.
- 휴대폰 브라우저는 https 에서만 카메라를 열어 주므로 스스로 만든 인증서(rcgen, 자료 폴더 `phonecam-cert.der · phonecam-key.der`, 한 번 만들어 계속 씀)로 https(tokio-rustls, ring) 서버를 연다. 처음 한 번 '안전하지 않음' 안내를 넘겨야 한다.
- 이어 주기(시그널링)만 서버가 한다: 휴대폰은 0.7초마다 `POST /cam/poll`(살아 있음 + offer · bye 받기), 답은 `POST /cam/answer`. 출력 화면은 이 PC 서버의 `POST /phonecam/offer`(X-WorshipCue 머리글) → `GET /phonecam/answer?viewer=` 폴링 → `POST /phonecam/bye`.
- ICE 는 STUN 없이 같은 내부망 주소만. 크롬이 가린 `xxxx.local` 주소는 서버가 실제 IP 로 바꿔 적는다(출력 쪽 offer → 휴대폰과 같은 대역의 이 PC 주소, 휴대폰 answer → 휴대폰이 보낸 요청의 IP).
- 카메라 장치 id `phone:<휴대폰 id>`. `Core::camera_choices` 에 연결된 휴대폰이 '휴대폰 · 이름' 으로 들어가므로 카메라 순서 · 템플릿 배경 · 화면 층 어디서나 고를 수 있다. render.js `cameraStream` 이 phone: 이면 WebRTC 로 받고, 끊기면 3초마다 다시 잇는다(같은 MediaStream 에 트랙만 바꿔 끼움).
- 휴대폰 화면: 앞·뒤 카메라, 1080p/720p, 이름, 화면 꺼짐 막기(Wake Lock), 해상도 유지 · 최대 8Mbps.
- 점검: `node tools/phonecam/e2e.js --key <열쇠>` — 가짜 카메라 엣지를 '휴대폰'으로, 다른 엣지의 /outputs/main 이 받아 그림 크기를 본다. 2026-11-26 1920x1080 받음 · 닫으면 휴대폰 쪽도 정리 확인.
- 한계: 내부망 출력(다른 PC 의 OBS)에서는 아직 휴대폰 영상을 받지 못한다(받는 길이 이 PC 서버에만 있음). 실제 휴대폰(안드로이드 크롬 · 아이폰 사파리)으로는 아직 시험하지 않았다.
