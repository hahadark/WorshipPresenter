# 웹판 4단계 B — 카메라 · 휴대폰 카메라 · PDF · 영상 · 유튜브 (계약 v1, 구현함)

`architecture.md` 9절 `web-media`. 조작 화면(egui) 안의 창은 `web-egui` 몫이고, 여기는 **출력 페이지 쪽 부품 · JS/HTTP 부품 · 교회 중계실의 신호**다.
코드: `cloud/src/media/` (`routes.mjs` 길 · 휴대폰 링크 · 시계 맞추기 · 이름 맞추기, `signal.mjs` 중계실 안의 휴대폰 신호, `index.mjs` 휴대폰 페이지 · 링크) ·
`cloud/client/media/` (`media.js` 출력 화면 부품, `phone.js` 휴대폰 페이지 부품, `cameras.html/js` 카메라 맞추기 화면, `pdf.mjs` · `assets-client.mjs`).
시험: `cloud/test/media.test.mjs`(단위 7) · `cloud/test/media-e2e/media.test.mjs`(끝에서 끝 3, CI) · `cloud/scripts/media-browser-e2e.mjs`(헤드리스 엣지 + 가짜 카메라 20, 화면 그림 — CI 밖).

**web/ 은 한 줄도 고치지 않았다.** 출력 페이지(`web/output`) · 휴대폰 페이지(`web/phone/cam.html`) · 렌더러(`web/shared/render.js`)를 그대로 쓰고, 클라우드에서 연 페이지에만 부품을 끼워 브라우저 기능(`getUserMedia` · `RTCPeerConnection` · `fetch`)을 바꿔 끼운다.
설치판(`crates/`)도 고치지 않았다.

## 1. 길과 메시지

| 길 | 권한 | 내용 |
|---|---|---|
| `POST /api/v1/media/phone-link` `{name?}` | write (주인 · 사람 편집자 · 설치판) | `{ok, url:'…/phonecam?k=ph1.…', expiresAt}` — 휴대폰을 카메라로 쓰는 링크(QR 로). 24시간 |
| `GET /phonecam?k=` | 링크 | 설치판의 휴대폰 페이지(`web/phone/cam.html`) 그대로 + `phone.js`. 링크가 틀리거나 끝났으면 403 안내 |
| `WS /rtc/phone?k=` | 링크 | 휴대폰 ↔ 교회 중계실 (아래 메시지) |
| `GET /rtc/phones` | view | `{ok, phones:[{id, name, size, viewers, since}]}` — 붙어 있는 휴대폰 |
| `GET /cameras` | view | 이 PC 의 카메라를 교회 카메라 이름에 맞추는 화면 + 휴대폰 링크 만들기 |
| `/cloud/media/pdf.mjs` · `assets-client.mjs` | (정적) | 웹 조작 화면이 `import` 하는 모듈 |
| `/cloud/vendor/pdfjs/` | (정적) | pdf.js 6.5.438 고정판(Apache-2.0) + cmaps · standard_fonts · wasm · iccs |

`/media/…` 는 1단계 설치판 파일 중계 길이라 피했다(주의: 길 종류 `'media'` 도 그쪽 것 — 이 모듈은 `'media-api'`).

휴대폰 신호 (설치판 `phonecam.rs` 의 HTTP 폴링을 실시간 연결 메시지로):

```
휴대폰 → 중계실   {t:'hello', id, name, size, viewers}      5초마다 · 바뀔 때만 (설치판은 0.7초 폴링 — 클라우드에서는 요청 수)
                  {t:'answer', viewer, sdp, to}
중계실 → 휴대폰   {t:'ready', v:1, helloMs} · {t:'offer', viewer, sdp, to} · {t:'bye', viewer}
출력 화면 → 중계실 (/events 연결로) {t:'rtc', op:'offer', phone, label, viewer, sdp} · {t:'rtc', op:'bye', phone, viewer}
중계실 → 출력 화면 {t:'rtc', op:'answer', viewer, sdp} · {t:'rtc', op:'offline', viewer}
```
- 출력 화면 쪽은 `render.js phoneStream` 이 부르는 `/phonecam/offer · answer · bye · list` 를 `media.js` 가 받아 이 메시지로 바꾼다. 휴대폰 쪽은 `cam.html` 의 `/cam/poll · /cam/answer` 를 `phone.js` 가 받는다.
- 휴대폰 찾기: id 로 먼저, 없으면 **이름**으로 (설치판에 등록한 휴대폰은 클라우드에서 id 가 다르다 — 사이트마다 저장소가 따로라 `cam.html` 이 새 id 를 만든다).
- 같은 휴대폰이 다시 붙으면 앞의 연결을 4001 로 닫는다. 휴대폰은 한 교회에 8대까지. SDP 한 개 60 KB 까지(중계실의 출력 화면 글 한도 4 KB 는 `rtc` 글만 넓힘).
- 휴대폰에 교회 열쇠 · PIN 을 주지 않는다: 링크는 `HMAC(AUTH_SECRET, 교회 · 만료 · 무작위)` 서명, 만료 뒤 4003. 지우는 기능은 없고 하루가 지나면 끝난다(비밀값을 바꾸면 모두 끝남).

## 2. 카메라를 '이름으로'

설치판은 카메라를 장치 id(`camera:<deviceId>`)로 고르는데, 이 id 는 **PC · 브라우저 · 사이트마다 다르다**(같은 PC 의 설치판 WebView2 와 크롬도 다름). 그래서 클라우드 출력 화면은:
1. 출력 자료의 `cameras[id].label`(설치판 카메라 설정의 이름 — 설치판이 이미 내보내던 칸)을 읽고,
2. 이 브라우저의 `/cameras` 에서 사람이 정한 짝(`localStorage['wp-cloud-camera-map']` = `{ 정리한 교회 이름 : 이 PC 장치 이름 }`)이 있으면 그 이름으로,
3. 없으면 같은 이름(정확히 → 대소문자 · 공백 · 끝의 `(046d:0825)` 무시 → 한쪽이 다른 쪽을 품음)의 장치를 찾는다. **id 가 아니라 이름을 저장**하므로 장치 id 가 바뀌어도 그대로 맞는다.
- 장치 이름은 카메라 허락이 있어야 보인다 → 처음 한 번 아무 카메라를 열었다 바로 닫는다.
- **OBS 브라우저 소스는 저장소가 크롬과 따로**다 → OBS 의 '상호작용' 으로 `/cameras` 를 한 번 열어 정하거나, 교회 카메라 이름을 장치 이름과 같게 둔다.
- 못 찾으면 그 칸은 검게 두고 개발자 콘솔에만 알린다(방송 화면에 글을 띄우지 않음).

**같은 카메라를 두 탭이 함께 열면** (확인함, 헤드리스 엣지 + 크롬 가짜 카메라): **같은 브라우저 안의 두 출력 탭은 둘 다 열린다** — 크롬이 한 장치를 브라우저 안에서 나눠 쓴다. 한 탭 안에서는 `render.js cameraStream` 이 이미 스트림 하나를 같이 쓴다.
다른 프로그램끼리(크롬 출력 탭 + OBS 의 카메라 소스, 또는 크롬 + OBS 브라우저 소스)는 Windows 에서 **카메라 종류에 따라 하나만 열리기도 한다**(UVC 장치를 한 프로세스가 차지 — Windows 11 의 카메라 프레임 서버 공유를 지원하는 장치는 함께 됨). 실제 카메라로는 확인하지 못했다: 이 PC 의 영상 장치는 'screen-recorder-device' 와 'OBS Virtual Camera' 뿐이고 둘 다 화면 내용을 내보낼 수 있어 찍지 않았다. → 교회 PC 에서 확인 필요. 설치판의 '카메라 동시 열기 시험'(`camtest`)과 같은 문제다.

## 3. 휴대폰 카메라 — NAT 와 TURN

영상은 클라우드를 지나지 않는다(WebRTC 로 휴대폰 → 출력 화면 직접). 클라우드는 신호(SDP)만 잇는다.

| 경우 | 되나 | 까닭 |
|---|---|---|
| 휴대폰 · 출력 PC 가 **같은 Wi-Fi** | 됨 (구현 · 시험함) | 내부망 주소(host 후보)로 바로. 휴대폰 페이지는 카메라 허락이 있어 실제 IP 를 낸다. 출력 화면(받기만)은 `.local` 가린 주소를 내는데, 휴대폰 크롬 · 사파리가 mDNS 로 풀어야 한다 — **실제 휴대폰으로는 확인 못 함**(설치판은 이 PC 가 주소를 알아 바꿔 적었지만 클라우드는 내부 주소를 모른다) |
| 다른 망이지만 보통 공유기(NAT) 뒤 | 대체로 됨 | 공개 무료 STUN `stun:stun.cloudflare.com:3478` 로 바깥 주소(srflx)를 알아 구멍 뚫기. `media.js` · `phone.js` 가 `iceServers` 가 빈 연결에 넣는다 |
| 휴대폰이 **LTE/5G**(통신사 CGNAT · 대칭 NAT) ↔ 교회 Wi-Fi | **TURN 이 필요할 때가 많다** | 양쪽이 대칭 NAT 면 STUN 구멍 뚫기가 안 됨 → 영상을 중계해 줄 TURN 서버가 있어야 함 |

**판단: 교회 안(같은 Wi-Fi)에서 쓰는 지금 용도에는 TURN 이 필요 없다.** 교회 밖 · 휴대폰 데이터로 보내려면 TURN 이 필요하다 → **켜지도 쓰지도 않았다**(사관님 결정).
Cloudflare Realtime TURN (2026-11-28 문서 확인):
- 값: 내보내는 데이터 1 GB 당 $0.05, **매달 처음 1,000 GB 무료**(SFU 와 함께 셈) — https://developers.cloudflare.com/realtime/sfu/platform/pricing/ · https://developers.cloudflare.com/realtime/turn/faq/ . 다만 TURN 개요 쪽(https://developers.cloudflare.com/realtime/turn/)은 "SFU 와 함께 쓰면 무료, 아니면 $0.05/GB" 로만 적혀 있어 문서끼리 어긋난다.
- 켜려면: 대시보드(또는 API)에서 **TURN 키를 만들고**, Worker 가 그 키의 API 토큰으로 짧은 자격(최대 48시간)을 `rtc.live.cloudflare.com/v1/turn/keys/<키>/credentials/generate-ice-servers` 에서 받아 `iceServers` 로 넘겨야 한다 — 계정 설정 · 비밀값이라 사관님 일.
- 한도: TURN 할당마다 대략 50~100 Mbps · 5~10 kpps (계정 전체가 아니라 연결마다). 1080p 휴대폰 영상(4~8 Mbps)이면 1시간에 2~4 GB → 무료 1,000 GB 로 한 달 수백 시간.
- STUN: "Cloudflare's STUN service at `stun.cloudflare.com` is free and unlimited" (https://developers.cloudflare.com/realtime/turn/faq/) — 계정 · 키 없이 씀.
- TURN 을 켜면 바꿀 곳: `src/media/routes.mjs ICE_SERVERS` 대신 Worker 가 받은 짧은 자격을 출력 · 휴대폰 페이지에 넘기는 길 하나(`/rtc/ice`)와 `media.js` · `phone.js` 의 `ICE`.

## 4. PDF

설치판 `pdf.rs` 는 Windows.Data.Pdf · 맥 PDFKit 으로 쪽 그림을 만들어 슬라이드쇼 순서로 넣는다. 웹은 **pdf.js** 로 같은 일을 한다:
```js
import { pdfToSlideshow } from '/cloud/media/pdf.mjs';
const { item, pages } = await pdfToSlideshow(file /* File · Blob · ArrayBuffer · 같은 사이트 주소 */, { title: '주보', width: 1920, onPage: (i, n) => … });
// item = { type:'slideshow', title, images:['/api/v1/assets/<id>', …], body:'', autoSec:0, autoLoop:false, countdown:'', pdf:true }  ← 설치판 pdf.rs 와 같은 모양
```
- 쪽마다 흰 바탕 PNG(가로 `width`) → **2C `POST /api/v1/assets`** 로 올림(같은 sha256 이 있으면 묻기만, `assets-client.mjs`) → 순서의 그림 주소는 `/api/v1/assets/{id}`. 출력 화면은 보통 그림 슬라이드로 보여 준다(출력 쪽에서 PDF 를 다시 그리지 않음).
- `upload` 를 바꿔 끼울 수 있다(오프라인: IndexedDB 에 두었다가 이어지면 올리기 — 6단계).
- pdf.js 는 **고정판을 이 사이트에서** 내보낸다(`package.json pdfjs-dist 6.5.438`, `npm run assets` 가 `public/cloud/vendor/pdfjs/` 로 복사). 예배 중 외부 CDN 을 부르지 않는다. 한글 PDF(CID 글꼴)용 cmaps · 끼워 넣지 않은 기본 글꼴 · JPEG2000/JBIG2 풀이(wasm)도 같은 곳.
- 웹 조작 화면의 CSP: `script-src 'self'`(모듈 · 워커 같은 곳), `isEvalSupported:false` 로 `unsafe-eval` 불필요. JPEG2000/JBIG2 그림이 든 PDF 는 wasm 을 쓰므로 `'wasm-unsafe-eval'` 이 있어야 그 그림이 나온다.
- 200쪽까지. 90 MiB 를 넘는 쪽 그림은 없다(1920px PNG 한 장 수 MB).

## 5. 영상 (R2 · 설치판 중계) · 탭끼리 맞추기 · 늦게 붙은 화면

- 출력 화면은 영상을 `<video src>` 로 재생하고 위치는 `render.js syncVideo` 가 코어 상태(`video {key, playing, offset, at, now}`)에 맞춘다 — 클라우드에서도 그대로. 주소가 **2C `/api/v1/assets/{id}?fallback=/media/…`**(R2, Range · ETag) 이든 1단계 `/media/…`(설치판 중계, Range)이든 같은 코드로 재생 · 넘기기가 된다(둘 다 시험함).
- **고친 결함 (1단계부터)**: 중계실은 출력 자료를 기억해 두었다가 늦게 붙은 화면에 그대로 줬는데, `syncVideo` · 무대 타이머는 자료 안의 `now`(만든 때 설치판 시계)와 받은 때의 차이를 'PC 시계 차이' 로 본다 → 10분 뒤에 붙은 화면은 영상 위치 · 남은 시간이 10분 틀렸다. 이제 중계실이 기억한 시간만큼 `video.now` · `timer.now` · 리모컨 `stage.timer.now` 를 앞으로 옮겨 준다(`routes.mjs freshen`, room.mjs 두 줄). 설치판 로컬 SSE 는 붙을 때마다 새로 만들어 이 문제가 없었다.
- **같은 PC 의 출력 탭끼리**(예: 같은 영상을 두 모니터로): `BroadcastChannel('wp-video-sync')` 로 가장 먼저 연 탭이 기준이 되어 0.5초마다 위치를 알리고, 다른 탭은 차이가 30 ms 넘으면 재생 속도(0.9~1.1)로 따라가고 1초 넘으면 바로 옮긴다. 시험: 두 탭 차이 25~32 ms. 다른 PC 끼리는 각자 코어 시계에 0.5초 안으로 맞춘다(설치판과 같음).
- 넘기기 확인: 두 탭 모두 6초 자리에 멈추고, 영상 요소가 실제로 그린 그림을 ffmpeg 의 6초 그림과 대조해 같음.

## 6. 유튜브

설치판의 유튜브 순서(`render.js` 임베드 `youtube-nocookie.com` + `syncYouTube` postMessage)가 **클라우드 출력에서 그대로 된다**(헤드리스 엣지에서 재생 화면 확인 — `artifacts/media-browser/E-youtube.png`).
클라우드 출력 페이지는 `Referrer-Policy: no-referrer` 이지만 임베드 iframe 에 `referrerpolicy="strict-origin-when-cross-origin"` 이 붙어 있어 유튜브가 요구하는 Referer 가 간다(없으면 '오류 153'). `origin=` 은 클라우드 주소가 된다.
**OBS 브라우저 소스는 실제로 돌려 보지 못했다** — OBS 는 크로미엄(CEF)이라 같은 임베드가 될 것으로 보지만, 소리(OBS 는 브라우저 소스 소리를 따로 섞음) · 자동 재생은 교회 PC 에서 확인 필요.

## 7. web-egui · web-offline 이 알아야 할 것

- **web-egui (웹 조작 화면)**
  - 카메라 순서를 만들 때 장치 id 와 함께 **이름(label)** 을 `settings.cameras[id].label` 로 꼭 둔다 — 다른 PC 의 출력은 이름으로 찾는다. 출력 화면용 카메라 목록은 `navigator.mediaDevices.enumerateDevices()`(그 PC 것)라 출력 PC 와 다를 수 있다 → '이 PC 카메라 맞추기' 로 `/cameras` 를 열어 주면 된다.
  - 휴대폰 카메라: QR 은 `POST /api/v1/media/phone-link` 의 `url` 로, 목록은 `GET /rtc/phones`. 휴대폰 장치 id 는 `phone:<id>` 그대로, 이름도 같이 저장.
  - PDF: `pdfToSlideshow` 를 단추 처리 안에서 부르고 나온 `item` 을 큐시트에 넣는다(설치판 PDF 순서와 같은 모양). CSP 는 4절.
  - 영상 · 그림 올리기: `assets-client.mjs uploadAsset(blob, name)` (90 MiB 이하 — 그 위는 2C 나눠 올리기).
  - 웹 조작 화면이 송출 주체로 영상 상태를 보낼 때 `video.now` 는 **보내는 순간의 시계**로(중계실이 늦은 화면에는 알아서 옮긴다).
  - 같은 PC 출력 탭은 `BroadcastChannel('wp-live')`(1단계)로 화면을 받고, 영상 위치는 `wp-video-sync` 가 맞춘다 — 조작 화면이 따로 할 것 없음.
- **web-offline (6단계)**
  - 휴대폰 카메라 신호는 교회 중계실을 지나므로 **인터넷이 끊기면 새 연결을 못 맺는다**(이미 맺은 WebRTC 는 같은 Wi-Fi 면 계속). 끊긴 동안은 설치판의 내부망 휴대폰 카메라(https 37560)를 쓰는 것이 길.
  - PDF 는 브라우저 안에서 그리므로 오프라인에서도 된다 — 올리기만 `upload` 를 IndexedDB 로 바꿔 두었다가 이어지면 2C 로.
  - pdf.js · 부품은 정적 파일이라 Service Worker 가 버전별로 담으면 된다: `/cloud/media/*`, `/cloud/vendor/pdfjs/**`(약 5 MB · 200여 파일).
  - 카메라 짝(`wp-cloud-camera-map`)은 localStorage 라 오프라인과 상관없다.

## 8. 확인 못 한 것

- 실제 카메라(교회 PC 의 USB 카메라 · 캡처보드) · 크롬과 OBS 가 같은 카메라를 동시에 여는 것 · 실제 휴대폰(안드로이드 크롬 · 아이폰 사파리)의 mDNS 풀이와 화질 · LTE 에서 TURN 없이 되는 비율.
- OBS 브라우저 소스에서 휴대폰 카메라 · 유튜브 · `/cameras` 짝 정하기.
- 한글 CID 글꼴 PDF(cmaps 를 넣었지만 손으로 만든 시험 PDF 는 Helvetica 만).
