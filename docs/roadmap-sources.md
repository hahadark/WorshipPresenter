# 소스 · 화면 구성 · 출력 — 로드맵과 설계 (2026-11-27)

주인(사관님)이 정한 방향: **"각 소스를 구분하고 조합해서 화면을 자유롭게 구성"**, "순서대로 진행".
이 문서는 1~4단계 전체를 내다본 자료 모델과, 지금(1단계)까지 한 것을 적는다. 2~3단계에서 모델을 다시 뜯지 않도록
3단계까지 쓸 칸을 미리 잡아 두었지만 **구현은 1단계만** 했다.

## (가) 로드맵

뼈대: **소스**(화면에 나올 수 있는 것) → **화면 구성**(소스를 칸에 배치) → **출력**(예배당 · 방송 · 무대가 각자 구성을 고름).

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 | 소스 정리: 성경 · 가사 · 공지 글, 카메라(장치별 · 휴대폰), 영상 파일, 유튜브, 그림, PDF · PPT, 이름 자막, 타이머 · 시계, 로고, 회중 메시지를 각각 이름 있는 소스로. 새 소스: 웹 페이지, 화면 캡처(모니터 · 창) | **이번 작업** (화면 캡처는 아래 (마) 결과대로) |
| 2 | 화면 구성 편집기: 캔버스에 칸을 놓고 소스를 넣음(겹침 순서 · 자르기 · 투명도). 기본 틀: 전체 · 좌우 나눔 · 작은 화면 · 아래 자막 · 3분할 | 모델만 |
| 3 | 출력마다 다른 구성: 순서마다 예배당/방송/무대 구성을 따로, 출력 추가 | 모델만 |
| 4 | 전환 · 자동화 (구성 사이 전환 효과, 순서 · 매크로 · 예약이 구성을 바꿈) | 위험만 |
| 5 | OBS 없이 직접 송출 | 미정 |

**시험 기준 화면**: "방송 = 설교자 카메라 전체 + 아래 성경 자막, 예배당 = 성경 본문 전체".
1~3단계가 끝나면 이것이 *설정만으로* 되어야 한다. 아래 모델로 쓰면 이렇게 된다 (3단계 끝 모습):

```json
"scenes": [
  { "id": "sc-cam-sub", "name": "카메라 + 아래 자막", "layers": [
      { "id": "l1", "source": "camera:<설교자 카메라 deviceId>", "rect": [0, 0, 1, 1] },
      { "id": "l2", "source": "live", "tpl": "overlay" } ] }
],
"outputScenes": { "main": "full", "overlay": "sc-cam-sub" }
```

## 지금 출력이 만들어지는 길 (1단계 전 · 읽은 것)

- 코어 `Core::output_payload(output)` (crates/wp-core/src/core.rs) 가 출력 하나(main = 예배당 · overlay = OBS 자막)에 보낼 JSON 을
  만든다: `template`(템플릿 하나 = 배경 · 글 모양 · 로고 · 움직임 · 전환), `slide`(LIVE 슬라이드), `layers`(배경 끄기 hideBg ·
  로고 화면 logo · 배경 미디어 media = 카메라/영상/그림), `overlays`(켠 오버레이 + 회중 메시지, trackb.rs overlays_for),
  `video`(영상 · 유튜브 재생 상태), `pip`(OBS 자막의 그림 우상단), `cameras`(카메라 화질 설정).
  무대 화면은 `stage_payload` 로 따로 (슬라이드 · 다음 · 타이머 · 메시지 · 고정).
- 이 JSON 은 `tokio::broadcast` → server.rs SSE `render` → 웹 `web/output/output.js` 가 받는다. 예배당 창(wp-output = WebView2)과
  OBS 브라우저 소스는 **같은 페이지**를 연다.
- output.js 는 render.js `buildStage(template)` 로 무대를 만들고 그 안에 층을 손으로 끼워 넣었다:
  `.bc-bg`(템플릿 배경 + 템플릿 로고) → `.bc-medialayer`(배경 미디어) → `.bc-vlayer`(순서 배경 영상) → `.bc-area`(본문, 전환 때 둘이 겹침)
  → `.bc-ovl`(오버레이, z 10) → `.bc-logoscreen`(로고 화면, z 20) → `.bc-draw`(그리기, z 50), 그리고 무대 밖 `#bcPip`.
  끼우는 자리를 `stage.children[1]` · `insertBefore` 로 정해서, 층 하나를 더하면 다른 층의 자리가 같이 흔들렸다.
- 카메라는 세 곳에서 열린다: 템플릿 배경 `background.type = camera`, 배경 미디어 `layers.media.type = camera`, 카메라 슬라이드 `kind = camera`.
  render.js 가 한 페이지 안에서는 `cameraStream(deviceId)` 하나를 같이 쓰고, 4초 안 쓰이면 닫는다. 휴대폰은 `phone:<id>` 를 WebRTC 로.
- 조작 화면 미리보기 · LIVE 는 egui `paint.rs paint_slide` 가 같은 규칙으로 직접 그린다 (배경 → 템플릿 로고 → 본문 → 로고 화면).
  카메라 · 영상 · 오버레이는 egui 에서 그리지 않는다 (예전부터).

## (나) 자료 모델

### 소스 (Source)

"화면에 나올 수 있는 것 하나". 모든 소스는 **문자열 식별자** 하나로 부른다. 구성(scene)의 칸은 이 식별자만 가리킨다.

| 종류 kind | 식별자 | 설정 (어디에 저장) | 상태 (코어가 계산, 저장 안 함) |
|---|---|---|---|
| live — 지금 송출 중인 순서 슬라이드 (성경 · 가사 · 공지 · 제목 · 그림 · PDF · PPT · 영상 · 유튜브 · 카메라 슬라이드) | `live` | 없음 (큐시트 순서 · 템플릿이 정함). 칸에서 `tpl`(main / overlay / 템플릿 id) 를 고를 수 있게 (2단계) | 지금 종류(content: bible · lyrics · notice · title · image · score · video · youtube · camera · blank), 제목, 보이는지 |
| background — 템플릿 배경 (색 · 그라데이션 · 그림 · 영상 · 카메라 · 움직임 · 어둡게 + 템플릿 로고) | `background` | 템플릿 `background` · `logo` (그대로) | 배경 끄기 hideBg |
| media — 배경 미디어 (미디어 패널에서 띄운 카메라 · 영상 · 그림) | `media` | 없음 (`layers.media`, 실행 중 상태) | 무엇이 떠 있는지 |
| slide-bg — 순서 배경 영상 | `slide-bg` | 순서 `bg` | 재생 중인지 |
| overlay — 오버레이 하나 (글 · 로고 · 시계 · 카운트다운 · 이름 자막) | `overlay:<오버레이 id>` | 설정 `overlays[]` (그대로) | on · 나가는 출력 |
| message — 회중 메시지 | `message` | 설정 `messageTemplates` | 띄웠는지 |
| logo-screen — 로고 화면 | `logo-screen` | `logoScreenImage` | 켰는지 |
| draw — 그리기 · 강조 | `draw` | 없음 | 선이 있는지 |
| pip — OBS 자막의 그림 우상단 | `pip` | `output.obsPip` | — |
| camera — 카메라 장치 하나 (휴대폰 포함) | `camera:<deviceId>` · `camera:phone:<id>` | 설정 `cameras[deviceId]` (화질 · 밝기, 그대로) | 어느 출력이 쓰고 있는지 |
| web — 웹 페이지 **(새)** | `src-<임의>` | 설정 `sources[]` `{id, kind:"web", name, url, on, outputs, place, opacity, fit}` | on · 나가는 출력 |
| capture — 화면 캡처 **(새, (마) 참고)** | `src-<임의>` | 설정 `sources[]` `{kind:"capture", target:{type:"screen"|"window", name}}` | — |

- 앞의 아홉은 **내장 소스**: 식별자가 고정이고 지울 수 없다. 지금 있는 기능을 이름만 붙여 모은 것이라 저장 형식이 바뀌지 않는다.
- 카메라는 장치별 소스다. 1단계에서는 목록에 보이기만 하고(어느 출력에서 쓰고 있는지), 2단계 구성 편집기에서 칸에 넣는다.
- **사용자 소스**(웹 · 캡처, 앞으로 RTMP · NDI 같은 것)는 설정의 새 배열 `sources` 에 덧붙인다. 1단계에서는 구성 편집기가 없으므로
  오버레이처럼 `on` + `outputs`(main · overlay) + `place`(`under` = 글 아래 · `over` = 글 위) 로 기본 구성에 들어간다.
  2단계에서 구성에 직접 넣으면 `on` · `place` 는 "기본 구성에서만 쓰는 값"으로 남는다 (지우지 않음).
- 영상 파일 · 그림 · 유튜브 · PDF · PPT 는 지금 "순서(cue item)"로 나온다 → 1단계에서는 `live` 소스의 내용 종류로 보이고,
  미디어 패널에서 띄운 것은 `media` 소스로 보인다. 2단계에서 "파일 소스"(`src-` + kind video/image/youtube/pdf)를 덧붙이면
  같은 그리기 코드(render.js buildContent / buildMediaLayer)를 그대로 쓴다.

### 화면 구성 (Scene)

```json
{ "id": "full", "name": "전체 화면 (기본)", "w": 1920, "h": 1080,
  "layers": [ { "id": "background", "source": "background" }, { "id": "media", "source": "media" }, ...,
              { "id": "l7", "source": "camera:abc", "rect": [0.6, 0.6, 0.38, 0.38], "crop": [0,0,0,0], "opacity": 1, "visible": true, "fit": "cover" } ] }
```

- `layers` 는 **아래에서 위로** 겹치는 차례. 칸(layer)마다 `source` 하나. 같은 소스를 두 칸에 넣을 수 있다 (같은 카메라를 크게 · 작게).
- `rect` = [x, y, w, h] 무대 비율(0~1), 없으면 전체. `crop` = [왼, 위, 오른, 아래] 비율. `opacity` 0~1. `fit` = cover / contain.
  `tpl`(live · background 칸만) = 이 칸에 쓸 템플릿 (`main` · `overlay` · 템플릿 id). 1단계 기본 구성은 이 값을 하나도 쓰지 않는다 → 지금과 같음.
- **기본 구성 `full`** (코어가 만들고 저장하지 않음): `background → media → (사용자 소스 under) → slide-bg → live → (사용자 소스 over) → overlay:<id>… → message → logo-screen → draw`,
  OBS 자막(overlay 출력)이면 맨 위에 `pip`. 이 차례가 1단계 전 output.js 가 손으로 끼우던 차례와 같다.
- 2단계 기본 틀은 이 모양에서 rect 만 다르게 만든다: 전체 · 좌우 나눔(0~0.5 / 0.5~1) · 작은 화면(오른쪽 아래 0.3) · 아래 자막(live 를 tpl overlay) · 3분할.

### 출력 (Output) 과 출력별 지정 — 3단계

```json
"outputScenes": { "main": "full", "overlay": "full", "stage": null },     // 설정: 출력마다 늘 쓰는 구성
"extraOutputs": [ { "id": "out-2", "name": "로비 화면", "scene": "sc-…", "monitor": 2 } ],
cue item: "scenes": { "main": "sc-…", "overlay": "sc-…" }                // 순서마다 (없으면 설정 값)
```

- 고르는 차례: 순서의 `scenes[출력]` → 설정 `outputScenes[출력]` → `full`. 템플릿을 고르는 지금 규칙(순서 tplMain/tplOverlay →
  activeTemplates → overlayLyrics)은 **칸의 `tpl` 이 비었을 때** 그대로 쓴다. 그래서 구성을 하나도 안 만든 사람에게는 아무것도 안 바뀐다.
- 무대 화면(stage)은 지금처럼 따로 (stage.html). 3단계에서 `outputScenes.stage` 가 있으면 무대 화면 대신 구성을 그린다.

### 출력 데이터 (payload) 에 덧붙인 것 — 1단계에서 구현

`output_payload(output)` 에 두 칸을 **덧붙였다** (옛 칸은 하나도 바꾸지 않음):

```json
"scene":   { "id": "full", "layers": [ { "id": "background", "source": "background" }, … ] },
"sources": { "src-ab12": { "kind": "web", "name": "주보", "url": "https://…", "opacity": 1, "fit": "cover" } }
```

`sources` 에는 그리는 데 설정이 필요한 사용자 소스만 담는다 (내장 소스는 지금 칸 `template` · `slide` · `layers` · `overlays` 를 그대로 쓴다).
`scene` 이 없으면(옛 코어) output.js 는 같은 기본 차례로 그린다.

## (다) 기존 템플릿 · 큐시트 · 설정을 그대로 살리는 규칙 (옮김 규칙)

1. **아무것도 옮기지 않는다.** 템플릿(`templates[]` 의 background · logo · font …), 큐시트 순서(tplMain · tplOverlay · bg · actions …),
   설정(overlays · activeTemplates · cameras · output.obsPip …)은 읽는 방법도 저장 형식도 그대로다. 소스 · 기본 구성은 코어가
   이 값들에서 **그때그때 계산**한다 (저장하지 않음).
2. 새 설정 키는 **덧붙이기만**: 1단계 `sources`(배열, 기본 `[]`). 2단계 `scenes`, 3단계 `outputScenes` · `extraOutputs`, 순서의 `scenes`.
   옛 판(2.0.x)은 settings.json 을 `serde_json::Value` 로 읽고 모르는 키를 그대로 두므로 다시 열어도 죽지 않고, 새 판으로 돌아오면 남아 있다.
   (2.0.x 의 기본값 합치기 deep_merge_defaults 는 모르는 키를 지우지 않는다 — 시험 `sources::tests::old_settings_keep_unknown_keys`.)
3. 템플릿 하나가 갖던 배경 · 글 · 로고 · 오버레이는 이제 "`background` 소스 + `live` 소스 + `overlay:*` 소스 + 기본 구성 `full`" 로 읽는다.
   템플릿을 고치면 그 소스들이 바뀐다 — 사용자가 보는 템플릿 편집기는 그대로다.
4. 2단계 이후에도 "구성을 안 만든 사람" 은 늘 `full` 을 쓴다. 구성을 지우면 `full` 로 돌아간다 (없는 구성 id → `full`).
5. 없는 소스를 가리키는 칸(지운 웹 소스 · 빠진 카메라)은 그리지 않고 건너뛴다 (오류 아님).

## (라) 단계별 바꿀 곳과 위험

### 1단계 (이번) — 바꾼 곳

| 곳 | 바꾼 것 |
|---|---|
| `crates/wp-core/src/sources.rs` (새) | 소스 목록(`catalog`) · 기본 구성(`default_scene`) · 출력 데이터의 `scene` · `sources` · 상태(어디에 나가는지) · 사용자 소스 저장/켜기/지우기 · 주소 검사 |
| `core.rs output_payload` | `scene` · `sources` 덧붙임 |
| `web/output/output.js` | 무대를 **소스마다 층 하나**(`.wp-layer[data-source]`)로: 구성의 칸 차례대로 빈 층을 만들고, 배경 · 미디어 · 본문 · 오버레이(하나씩) · 로고 화면 · 그리기가 자기 층에만 그린다. `insertBefore(stage.children[1])` 같은 자리 계산이 없어졌다. 웹 소스(iframe) 그리기 |
| `web/shared/render.js` | 바꾸지 않음 — output.js 가 오버레이마다 `buildOverlays([o])` 를 불러 층 하나씩 (함수 그대로), 칸 모양은 output.js `layerBox` |
| `crates/wp-app/src/paint.rs` | `paint_slide` 를 기본 구성 차례(배경 → 템플릿 로고 → 본문 → 로고 화면)대로 층 함수를 부르게 나눔 (그림 결과 같음) |
| `crates/wp-app/src/sources_ui.rs` (새) | 인스펙터의 '소스' 탭: 묶음(화면 내용 · 자막 · 오버레이 · 카메라 · 웹 페이지) · 지금 어디에 나가는지(예배당 · OBS). 출력 화면이 붙어 있고 나가는 중이면 빨강, 출력 화면이 꺼져 있으면 흐린 글자 · 웹 페이지 소스 더하기 · 켜기 · 출력 · 글 위/아래 · 지우기. 점검 인자 `--ins-tab sources` |

층 상자(`.wp-layer`)는 `position:absolute; inset:0` 이고 z-index · opacity · transform 을 주지 않는다 → 쌓임 맥락(stacking context)을
새로 만들지 않아서, 안의 `.bc-ovl`(z 10) · `.bc-logoscreen`(z 20) · `.bc-draw`(z 50) 의 앞뒤가 전과 같다. rect · crop · opacity 를
줄 때(2단계)만 상자에 스타일이 붙는다.

### 1단계 위험

- 웹 출력의 DOM 이 한 겹 깊어졌다: `:scope > .bc-area` · `stage.firstChild` 처럼 무대 바로 아래를 찾던 곳을 층 안에서 찾게 고쳤다.
  render.js `renderInto`(조작 화면이 아닌 다른 쪽 미리보기용)는 그대로 두었다 (층 없이 예전 모양).
- OBS 브라우저 소스는 OBS 에 따라 CEF 판이 다르다 — iframe(웹 소스)은 어디서나 되지만, 그 사이트가 `X-Frame-Options`/`frame-ancestors` 로 막으면 빈 칸이 된다 (조작 화면에 안내).

### 2단계 — 화면 구성 편집기

- 바꿀 곳: 설정 `scenes` 저장/검사(sources.rs), 조작 화면 구성 편집기(캔버스 · 칸 끌기 · 겹침 · 자르기 · 투명도 · 기본 틀 5개),
  output.js 층 상자에 rect/crop/opacity 적용(자리는 이미 있음), paint.rs 가 칸마다 그리기(카메라 · 웹은 egui 에서 자리 표시만).
- 위험: ① 글자 크기 맞춤(fit)은 템플릿 전체 크기 기준이다 → 작은 칸에 live 를 넣으면 무대를 칸 크기로 따로 만들어 줄여야 한다
  (칸마다 `buildStage` · `scaleStage` 를 한 번 더, 측정 기준이 달라짐). ② egui 미리보기와 웹 출력의 parity 가 칸마다 늘어남 → parity 사례 추가.
  ③ 같은 카메라를 두 칸에 넣으면 페이지 안에서는 스트림 하나를 같이 쓴다(이미 그렇게 됨).

### 3단계 — 출력마다 다른 구성

- 바꿀 곳: `outputScenes` · 순서 `scenes` · 추가 출력(`/outputs/<id>`, wp-output 창 여러 개), 조작 화면 뷰어(예배당 · OBS 탭 옆에 추가 출력).
- 위험: ① **카메라 동시 사용** — 예배당 창(WebView2)과 OBS(CEF)는 다른 프로세스라 같은 카메라를 각자 연다 → (바) 확인 결과와 대책.
  ② 소리: 영상 소리는 지금 예배당 창에서만 낸다(OBS 는 무음). 출력이 늘면 "소리 내는 출력" 하나를 정해야 한다.
  ③ 템플릿 고르는 규칙과 구성의 `tpl` 이 겹친다 — 칸에 `tpl` 이 없을 때만 옛 규칙.

### 4단계 — 전환 · 자동화

- 바꿀 곳: 구성 바꿀 때 전환(디졸브는 층 단위로 이미 있음 → 구성 단위로), 순서 '송출할 때 자동으로'(item.actions)에 `scene`, 매크로 · 예약 · /api/trigger/scene.
- 위험: 구성이 바뀔 때 같은 소스(카메라 · 유튜브 · 웹)는 다시 열지 않고 층을 옮겨야 끊기지 않는다 → 층 열쇠를 소스 id 로 두었다(1단계에서 그렇게 만듦).

## (마) 화면 캡처 — 확인 결과 (2026-11-27, 이 PC · WebView2 런타임 · wp-output.exe 로 실제 시험)

**결론: 예배당 창(WebView2)에서 사용자 허락 창 없이 화면을 캡처할 방법이 없다 → 1단계에 넣지 않았다.**
설정 모델에는 `kind: "capture"` 자리만 남겨 두었다 (코어가 아직 받아들이지 않음, `USER_KINDS = ["web"]`).

| 시험 | 결과 |
|---|---|
| `getDisplayMedia()` 기본 | WebView2 가 자기 고르기 창("http://127.0.0.1:…과(와) 공유할 항목 선택", Window / 전체 화면 탭)을 띄우고 사람이 누를 때까지 멈춘다 (화면 찍어 확인) |
| 같은 것 + `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--auto-select-desktop-capture-source=Entire screen --use-fake-ui-for-media-stream"` | 똑같이 고르기 창이 뜬다 — 크롬 시험용 자동 고르기 플래그가 WebView2 에서는 듣지 않음 |
| 옛 방식 `getUserMedia({ video: { mandatory: { chromeMediaSource: 'desktop', chromeMediaSourceId: 'screen:0:0' } } })` (Electron · CEF 에서 허락 창 없이 되는 길) | `AbortError: Error starting tab capture` 로 거절 |
| wry 권한 처리기 | WebView2 는 화면 캡처를 PermissionRequested 로 묻지 않아 `PermissionKind::DisplayCapture` 로 허락할 수 없다. ICoreWebView2_27 `ScreenCaptureStarting` 은 막기만 할 수 있고 고르기 창을 건너뛰지 못한다 |
| OBS 브라우저 소스 (CEF) | OBS 가 화면 캡처를 이미 직접 한다 (디스플레이 · 창 캡처 소스). 우리 페이지 안의 iframe 은 `display-capture` 권한 정책으로 막힘을 확인 |

허락 창을 한 번 누르게 하는 것은 예배 중 무인 출력에 맞지 않고, 다시 켤 때마다 또 묻는다. 대안 (2단계 이후, 필요할 때):

1. **네이티브 캡처 → 로컬 스트림 (추천)**: wp-app(또는 작은 도우미 exe)이 Windows.Graphics.Capture(모니터 · 창, 허락 창 없음, 노란 테두리는 Win11 에서 끌 수 있음)로
   프레임을 받아 → ① 127.0.0.1 MJPEG/`multipart/x-mixed-replace` 로 내보내 출력 페이지가 `<img src="/capture/<id>">` 로 받기(가장 쉬움, CPU 조금),
   또는 ② WebRTC 로컬 연결(휴대폰 카메라 경로 `phonecam` 과 같은 받기 코드 재사용, 화질 좋음). 맥은 ScreenCaptureKit (화면 기록 권한 한 번).
2. **OBS 에 맡기기**: 방송 화면에서는 OBS 의 디스플레이/창 캡처 소스를 쓰고, 우리 구성에는 넣지 않는다 (예배당 화면에는 안 나옴).
3. 예배당 창을 띄울 때 한 번 고르게 하기 — 위 이유로 비추천.

## (바) 카메라 동시 사용 — 확인 결과 (2026-11-27)

이 PC 에는 실제 카메라(UVC)가 없다 (장치 목록: 스캐너뿐). 그래서 **가상 카메라(OBS Virtual Camera, OBS 33.0.1 이 설치한 DirectShow 장치)** 로 시험했다.
OBS 를 켜지 않은 상태라 그 장치는 안내 그림을 1920×1080 60fps 로 내보낸다.

| 시험 | 결과 |
|---|---|
| wp-output.exe(WebView2) 둘을 서로 다른 자료 폴더(= 다른 브라우저 프로세스)로 동시에 열고 같은 장치 `getUserMedia` | 둘 다 열림 · 둘 다 1920×1080 · 영상 시간이 2초마다 늘어남(프레임 들어옴), 끊김 없음 |
| wp-output.exe(WebView2) + 크롬(헤드리스, OBS 브라우저 소스와 같은 Chromium 캡처 코드 — OBS CEF 대신) 동시에 같은 장치 | 둘 다 열림 · 둘 다 프레임 들어옴 |

**한계 — 이것으로 실제 카메라가 된다고 말할 수 없다.** 가상 카메라는 공유 메모리에서 읽기만 해서 여러 앱이 동시에 여는 것이 원래 된다.
실제 웹캠 · 캡처보드(UVC)는 Windows 의 Media Foundation 이 장치를 **한 프로세스만** 잡게 하는 경우가 많고(두 번째는 `NotReadableError: Could not start video source`),
Windows 10 1809+ 의 Camera Frame Server · Windows 11 의 '여러 앱에서 카메라 동시 사용' 설정이 켜져 있거나 드라이버가 공유를 지원하면 된다. 장비마다 다르다.
교회 PC 의 실제 카메라로 같은 시험을 해야 한다 (시험 페이지: 출력 창 `wp-output --helper --url <페이지>` 와 OBS 브라우저 소스에서 같은 장치를 동시에 열기).

**지금 이미 있는 것**: 한 페이지 안에서는 같은 카메라를 한 번만 연다 (render.js `cameraStream` 공유). 예배당 창(main)과 OBS 자막(overlay)은
다른 프로세스라 각자 연다 → 같은 카메라를 두 곳에 쓰면 실제 장비에서 한쪽이 '카메라 없음' 이 될 수 있다 (예전부터 있던 위험).

**설계상 대책 (3단계에서 출력마다 다른 구성을 넣을 때 함께)**: "한 곳에서 열어 로컬로 나눠 주기".
1. 카메라를 여는 곳을 하나로: 예배당 창(wp-output, 늘 켜져 있는 쪽)이 카메라를 열고, 다른 출력(OBS 브라우저 소스 · 추가 출력)은
   `camera:<id>` 소스를 **127.0.0.1 WebRTC** 로 받는다. 휴대폰 카메라(`phone:<id>`)가 이미 이 모양이다: 보내는 쪽 → 코어가 연결 정보만 잇고(`/phonecam/offer` · `answer`)
   → 받는 쪽 render.js `phoneStream`. 같은 코드를 "출력 창이 보내는 쪽" 으로 쓰면 된다 (`cameraStream` 이 장치를 못 열면 이 길로 바꿔 받기).
2. 예배당 창이 꺼져 있을 때: 숨은 도우미(wp-output `--helper`, 카메라 찾기에 이미 씀)가 보내는 쪽을 맡는다.
3. OBS 에서만 카메라를 쓰는 교회는 OBS 의 비디오 캡처 장치 소스를 그대로 쓰는 편이 화질 · 지연에 낫다 → 구성에서 '방송' 카메라 칸을 비워 두는 길도 안내.

## 1단계 확인 (2026-11-27)

- 시험: wp-core 321 통과(19 제외, 새 6개: `sources::tests`) · wp-app 233 통과(1 제외).
- parity(`tools\parity
un.ps1` + `verify.js`): 46 경우 중 정적 44, 글자 크기 같음 42/44 — 다른 둘(obs-pip-image 72/46 · obs-pip-score 72/56)은
  이번 paint.rs 를 2.0.8 원본으로 되돌려 돌려도 같은 값 · 같은 화소 비율이라 이번 변경과 무관 (2.0.8 에 이미 있음).
- 실제 웹 출력(output.js) 전후 비교: 같은 자료 · 요한복음 3:16 송출 · 오버레이 둘(글 · 이름 자막)로 `/outputs/main` · `/outputs/overlay` 를 1920×1080 으로 찍어
  2.0.8 output.js 와 새 output.js 를 비교 → **두 출력 모두 다른 화소 0개** (문턱 0). DOM 은 `.wp-layer` 가 소스마다 하나 (background · media · slide-bg · live · overlay:ovt · overlay:ovn · message · logo-screen · draw).
- 웹 페이지 소스: 켜면 `src-…` 층이 live 위 · 오버레이 아래(place over)에 iframe 으로 나옴 확인.

## (사) 2단계를 시작하기 전에 주인이 정할 것

1. **구성 편집기가 놓일 곳**: 별도 창(템플릿 편집기처럼) / 인스펙터 '소스' 탭 아래 / 가운데 뷰어 자리를 잠깐 바꿔 쓰기. (추천: 별도 창 — 캔버스가 넓어야 칸을 끌기 쉽다)
2. **글자 칸(live)을 작은 칸에 넣을 때 글자 크기**: 칸 크기에 맞춰 다시 맞추기(템플릿 글자 크기를 칸 비율만큼 줄임) / 템플릿 크기 그대로 잘라 보이기. (추천: 다시 맞추기)
3. **조작 화면 미리보기에서 카메라 · 웹 페이지 칸**: 지금처럼 자리 표시(이름 상자)만 / 실제 영상까지 (egui 에 영상을 그리려면 따로 일이 큼). (추천: 자리 표시)
4. **화면 캡처를 할지**: (마)의 네이티브 캡처 → 로컬 스트림을 2단계에 넣을지, OBS 에 맡길지.
5. **카메라를 여는 곳 하나로 모으기**(바 대책)를 3단계 전에 할지 — 교회 PC 의 실제 카메라로 동시 열기를 먼저 시험해 보고 정하면 된다 (안 되면 꼭 필요).
