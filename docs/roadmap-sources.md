# 소스 · 화면 구성 · 출력 — 로드맵과 설계 (2026-11-27)

주인(사관님)이 정한 방향: **"각 소스를 구분하고 조합해서 화면을 자유롭게 구성"**, "순서대로 진행".
이 문서는 1~4단계 전체를 내다본 자료 모델과, 지금(1단계)까지 한 것을 적는다. 2~3단계에서 모델을 다시 뜯지 않도록
3단계까지 쓸 칸을 미리 잡아 두었지만 **구현은 1단계만** 했다.

## (가) 로드맵

뼈대: **소스**(화면에 나올 수 있는 것) → **화면 구성**(소스를 칸에 배치) → **출력**(예배당 · 방송 · 무대가 각자 구성을 고름).

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 | 소스 정리: 성경 · 가사 · 공지 글, 카메라(장치별 · 휴대폰), 영상 파일, 유튜브, 그림, PDF · PPT, 이름 자막, 타이머 · 시계, 로고, 회중 메시지를 각각 이름 있는 소스로. 새 소스: 웹 페이지, 화면 캡처(모니터 · 창) | **이번 작업** (화면 캡처는 아래 (마) 결과대로) |
| 2 | 화면 구성 편집기: 캔버스에 칸을 놓고 소스를 넣음(겹침 순서 · 자르기 · 투명도). 기본 틀: 전체 · 좌우 나눔 · 작은 화면 · 아래 자막 · 3분할 | **됨 (2026-11-27, 아래 '2단계 한 것')** |
| 3 | 출력마다 다른 구성: 순서마다 예배당/방송/무대 구성을 따로, 출력 추가 | **됨 (2026-11-27, 아래 '3단계 한 것', 무대는 그대로)** |
| 4 | 전환 · 자동화 (구성 사이 전환 효과, 순서 · 매크로 · 예약이 구성을 바꿈) | **됨 (2026-11-27, 아래 '4단계 한 것')** |
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

  → 주인 결정 (2026-11-27): 1) 별도 창 2) 다시 맞추기 3) 자리 표시만 4) 화면 캡처는 이번에 안 함(OBS 에 맡김, 제안으로만) 5) 카메라 모으기는 교회 PC 시험 뒤 — 이번에 안 함.

## 2단계 한 것 (2026-11-27, sources 가지, 아직 릴리스 전)

### 쓰는 법

1. **화면 → 화면 구성 편집…** (또는 인스펙터 '소스' 탭 → 화면 구성 → '화면 구성 편집…'). 템플릿 편집기처럼 따로 뜨는 창.
2. 왼쪽 아래 **+ 새 구성** → 기본 틀 5개(전체 · 좌우 나눔 · 작은 화면 (구석) · 아래 자막 · 3분할) 가운데 하나. 카메라가 있으면 카메라 칸에 첫 카메라가 들어가 있다. 없으면 '소스 고르기' 빈 칸.
3. 칸을 누르고 오른쪽 **소스** 에서 넣을 것을 고른다 (송출 중인 순서 · 템플릿 배경 · 배경 미디어 · 순서 배경 영상 · 카메라 · 웹 페이지 · 오버레이 · 회중 메시지 · 로고 화면).
   송출 중인 순서 · 템플릿 배경 칸은 **글 모양** 도 고른다 (이 출력의 템플릿 그대로 / 예배당 템플릿 / OBS 자막 템플릿(아래 띠) / 아무 템플릿).
4. 캔버스에서 칸을 끌어 옮기고, 손잡이 8개로 크기를 바꾼다. 격자(24칸) · 화면 가운데 · 다른 칸 가장자리에 붙는다 (분홍 안내선). Alt = 안 붙음, 모서리 + Shift = 비율 그대로.
   숫자로: 자리 · 크기(무대 px) · 자르기(%) · 투명도 · 모서리 둥글기 · 테두리(두께 · 색) · 칸 이름. 칸 목록에서 눈(보이기) · 자물쇠(잠그기 = 캔버스에서 안 움직임) · 겹침 순서(맨 앞 · 앞 · 뒤 · 맨 뒤) · 지우기.
5. 위의 **쓰는 곳 [예배당] [방송 (OBS)]** 를 누르면 그 출력에 바로 나간다 (다시 누르면 기본 화면으로). 인스펙터 소스 탭의 고르기 칸으로도 고를 수 있다.
6. 바꾸면 0.35초 뒤 저절로 저장되고 출력에 바로 반영. **Ctrl+Z / Ctrl+Shift+Z · Ctrl+Y** 는 마우스가 창 위에 있으면 이 창의 되돌리기(큐시트 되돌리기가 아님).
7. 구성 이름 바꾸기(가운데 위 이름 칸) · 복제 · 지우기(한 번 더 묻기 — 쓰던 출력은 기본으로).

OBS 주소는 그대로 `/outputs/overlay` (예배당은 `/outputs/main`). 구성 전용 주소는 만들지 않았다 — 3단계 '출력 추가' 때 `/outputs/<id>` 로.

### 저장 형식 (덧붙이기만)

- 설정 `scenes[]`: `{ id: "sc-…", name, layers: [{ id: "l-…", source, rect:[x,y,w,h], crop?:[l,t,r,b], opacity, visible, locked, radius(px), border:{width,color}, fit, tpl, name? }] }` — rect · crop 은 비율, radius · border 는 무대(템플릿) px.
- 설정 `outputScenes`: `{ main?: id, overlay?: id }`. 없거나 지운 구성이면 기본 `full`.
- 옛 2.0.x 는 두 키를 모른 채 그대로 둔다 (deep_merge_defaults 는 모르는 키를 안 지움 — 1단계 시험 `old_settings_keep_unknown_keys` 가 scenes 까지 확인). 출력 데이터에는 `scene` 칸에 그 구성 + 맨 위 자동 칸(`auto: true`)을 실어 보내고, 칸의 `tpl` 은 템플릿 값(`template`)으로 바꿔 붙인다 (저장 안 함).

### 그리는 규칙 (웹 output.js = 조작 화면 paint.rs)

- 칸 = 무대(출력 템플릿 크기) 위의 상자. **상자가 필요 없는 칸**(전체 · 자르기 없음 · 같은 템플릿 · 둥글기/테두리 없음)은 1단계처럼 무대에 바로 → 구성을 안 고른 사람은 2.0.8 과 화소까지 같다.
- 상자가 필요한 칸: `render.js sceneHost` = 자르기만큼 키운 그릴 자리 + **칸 크기 템플릿**(`sceneTemplate` = wp-core `scenes::box_template`: 높이는 템플릿 그대로, 폭은 칸 비율) 을 scale 로 줄여 넣음 → 글자 크기는 칸 높이에 비례하고, 넘치면 fit 이 줄인다 (주인 결정 2 '다시 맞추기').
- 칸 차례 = 쌓는 차례. 사용자 구성에 없는 오버레이 · 회중 메시지 · 로고 화면 · 그리기 (OBS 면 그림 우상단) 는 맨 위에 저절로 (`scenes::effective_scene`). 같은 소스를 두 칸에 넣으면 송출 중인 순서 · 템플릿 배경 · 배경 미디어는 첫 칸에만, 카메라는 칸마다 (페이지 안에서 스트림 하나를 같이 씀).
- 구성에 직접 넣은 카메라 칸(`.wp-cam`)은 '배경 지우기' 에 숨지 않는다 (배경 미디어 카메라는 예전처럼 숨음).
- 조작 화면 미리보기 · LIVE: `paint::paint_scene` — 카메라 · 웹 페이지 칸은 이름 상자(주인 결정 3). 둥근 모서리는 egui 에서 잘라 내지 못해 테두리만 둥글게(안쪽 그림은 네모).
- 구성의 칸 자리 · 자르기 · 글 모양이 바뀌면 그 칸 상자만 새로 만들고 그 소스를 다시 그린다. 이미 있는 칸은 옮기지 않는다 (영상 · 카메라 끊김 방지).

### 확인 (2026-11-27)

- 시험: wp-core 327 통과(19 제외, 새 6개 `scenes::tests`) · wp-app 237 통과(1 제외, 새 4개: 끌기 계산 2 · 편집 창 headless 끌기 · 모서리 · 실행 취소/다시 · 자동 저장 · 잠근 칸 · 출력 데이터).
- 웹 출력 실제 확인 `tools/scenes/e2e.js` (앱을 임시 자료로 띄우고 머리 없는 엣지로 1920×1080):
  ① 구성을 안 고른 자료의 `/outputs/main` · `/outputs/overlay` = 2.0.8 output.js (c81e8a3 web\) 와 **다른 화소 0개** (문턱 0).
  ② 시험 기준 "방송 = 카메라 전체 + 아래 성경 자막, 예배당 = 성경 본문 전체": 방송 칸 차례 카메라 → 송출 중인 순서(OBS 자막 템플릿) 확인, 카메라 영상 1920×1080 들어옴, 예배당은 구성 고르기 전과 화소까지 같음.
  ③ 좌우 나눔(왼쪽 카메라 자르기 10% · 오른쪽 성경 둥글기 24 · 테두리 6)을 예배당에: 오른쪽 칸 글자 67px 로 다시 맞춤.
  카메라: 이 PC 의 OBS Virtual Camera 로 먼저 시험해 영상이 들어오는 것(472×254)까지 봤으나 그 가상 카메라가 화면 내용을 내보내고 있어 캡처를 지웠고, 보고용 캡처는 브라우저 시험 카메라(`--fake-cam`)로 다시 찍었다.
- parity (`tools\parity\run.ps1` + `verify.js`, 50 경우): 새 구성 사례 4개(좌우 나눔 · 작은 화면 둥글기/테두리 · 아래 띠 템플릿 · 3분할 자르기) 모두 글자 크기 같음, 화소 차이 0.4~2.6% (글자 가장자리, 기존 사례와 같은 수준). 전체 46/48 같은 글자 크기 — 다른 둘은 2.0.7 부터 있던 obs-pip-image · obs-pip-score.

### 3단계 전에 정할 것

1. **순서마다 구성**: 순서 편집칸에 '예배당 구성 · 방송 구성' 고르기를 둘지, 순서 '송출할 때 자동으로'(item.actions)에 '구성 바꾸기' 동작으로 둘지 (앞의 것은 그 순서에서만, 뒤의 것은 바꾼 채로 남음).
2. **출력 추가**(로비 화면 등)의 주소 모양 `/outputs/<id>` 와, 그 출력을 띄울 모니터 고르기를 설정 → 출력에 둘지.
3. **무대 화면도 구성으로** 그릴지 (지금은 stage.html 따로).
4. **소리 내는 출력** 하나 정하기 (영상 소리는 지금 예배당 창만).
5. **카메라 모으기**(바 대책): 교회 PC 의 실제 카메라로 예배당 창 + OBS 브라우저 소스 동시 열기 시험 결과에 따라.
6. 편집기에서 다른 칸에 가려진 칸은 목록에서 고르고 숫자로만 옮길 수 있다 — 캔버스에서 Alt+누르기로 아래 칸 고르기를 넣을지.

### 다른 갈래와 겹칠 곳 (ai-features)

- `web/output/output.js` (층 · 칸 상자 · 카메라 칸) · `web/shared/render.js` (sceneHost 등 함수 4개 덧붙임, 끝의 BCRender 내보내기 줄).
- `crates/wp-app/src/track_c.rs` (화면 메뉴에 '화면 구성 편집…', TrackC 에 scenes) · `trackb_ui.rs` 되돌리기 조건.
- 인스펙터 '소스' 탭(`sources_ui.rs`) 맨 위 '화면 구성' 구획. 인스펙터 탭 수는 그대로 (`prolayout_ui.rs` 안 바꿈).
- `app.rs` Snap 에 scenes · source_names 덧붙임, 뷰어 그리기 한 줄 (paint_slide → paint_scene).
- `crates/wp-core/data/en.json` 맨 앞 62줄.

## 3단계 한 것 (2026-11-27, sources 가지, 아직 릴리스 전)

주인 결정(2단계 '정할 것'): ① 순서의 구성은 인스펙터 '화면 구성' 구획, '송출할 때 자동으로' 와 따로 ② 출력 추가 · 주소 · 모니터는 출력 설정 창, 편집기 '쓰는 곳' 은 출력 목록을 따름 ③ 무대 화면은 그대로 ④ 소리는 예배당 하나 기본, 설정에서 바꿈 ⑤ 카메라 모으기는 안 하고 '카메라 동시 열기 시험' 도구 ⑥ 가려진 칸 고르기.

### 쓰는 법

1. **출력 더하기**: 화면 → 출력 설정 (또는 인스펙터 소스 탭 → '출력 설정…', 설정 찾기 '출력 추가'). '출력' 구획에서 이름을 넣고 [출력 더하기].
   출력마다: 이름 · **어느 출력처럼**(예배당처럼 / 방송(OBS)처럼 — 템플릿 고르는 규칙 · 오버레이 · 배경 미디어 · 그림 우상단을 그 출력과 같게) · 템플릿(비우면 그 출력의 템플릿) ·
   **기본 화면 구성** · 모니터(없음 = 주소로만) + [모니터에 띄우기] · 주소 `http://127.0.0.1:<포트>/outputs/out-…` [복사] · 지우기(한 번 더 묻기). 최대 8개.
   모니터에 띄운 창은 예배당 창과 같은 wp-output (창을 눌러 고른 뒤 Esc = 닫기 → '띄우기' 가 꺼짐). 내부망 출력도 같은 주소로 열린다.
2. **소리 내는 출력**: 같은 창 '영상 · 유튜브 소리' — 기본 예배당. 고른 출력이 모니터 창일 때만 소리가 난다 (2.0.8 과 같은 규칙: 브라우저 · OBS 브라우저 소스는 늘 무음).
3. **순서마다 출력별 구성**: 큐시트에서 순서를 고르고 인스펙터 [소스] 탭 → '화면 구성' 구획.
   - **고른 순서 · <제목>**: 출력마다 한 줄 — '따라감 → …'(지정 없음) / 전체 화면 (기본) / 구성.
   - **<종류> 순서 기본** (성경 · 찬양 · 찬송가·PPT · 공지 · 제목 · 그림 · 영상 · 카메라 · 슬라이드쇼 · 빈 화면): 그 종류 순서 모두에.
   - **출력 기본**: 출력이 늘 쓰는 구성 (2단계의 고르기와 같음).
   - 송출하면 출력마다 **순서 → 종류 기본 → 출력 기본 → 전체 화면** 차례로 고른다. 송출 중인 순서의 지정을 바꾸면 다시 송출하지 않아도 바로 바뀐다. 순서 지정은 실행 취소(Ctrl+Z)에 남는다.
4. **표시**: 큐시트 줄 오른쪽 · 예배 시간표 막대 오른쪽 위의 작은 격자 아이콘 = 이 순서는 어느 출력에서 출력 기본과 다른 구성을 씀 (마우스를 올리면 '방송 (OBS): 카메라 + 자막' 처럼).
5. **뷰어**: '다음 · 송출 중' 머리의 [예배당 | OBS] 가 출력 목록을 따른다 ([예배당 | OBS | 로비 화면 …]). 고른 출력의 템플릿 · 구성으로 그린다.
6. **구성 편집기**: '쓰는 곳' · '미리 보기' 가 출력 목록. 캔버스에서 **Alt+누름(끌지 않고) = 그 자리에 겹친 칸을 위에서 아래로 차례로 고르기** (맨 아래 다음은 다시 맨 위). 그렇게 고른 칸은 다음 끌기에서 맨 위 칸 대신 움직인다. **Alt+끌기 = 안 붙음** 은 그대로 (누름과 끌기는 따로라 겹치지 않음).
7. **연결 → 카메라 동시 열기 시험**: 카메라를 고르고 [시험 시작] → 예배당 출력 창(wp-output 작은 창, 같은 WebView2 프로필)이 먼저 열고 2.5초 뒤 기본 브라우저가 같은 카메라를 연다
   (브라우저는 deviceId 가 달라 이름이 같은 장치로, 허락을 물으면 '허용'). 곳마다 '열림 · 장치 이름 · 1920×1080 · 30fps · 화면 들어옴' 또는 '열 수 없음 — NotReadableError …' 를 1초마다 보이고,
   결론 한 줄과 [결과 복사]. OBS 에서도 보려면 창의 주소를 브라우저 소스로 (같은 결과 줄에 'OBS 브라우저 소스' 로 붙음). 3분 뒤 페이지가 스스로 카메라를 닫는다.

### 저장 형식 (덧붙이기만)

- 설정 `extraOutputs[]`: `{ id: "out-…", name, kind: "main"|"overlay", template: "", monitor: -1, window: false }` (모르는 칸은 그대로 둠).
- 설정 `typeScenes`: `{ <순서 종류>: { <출력 id>: <구성 id | "full"> } }`. 설정 `output.audioOutput` (없으면 예배당). `outputScenes` 는 추가 출력 id 도 받는다.
- 큐시트 순서 `scenes`: `{ <출력 id>: <구성 id | "full"> }` — 빈 묶음이면 칸 자체를 지운다(옛 모양 그대로). `"full"` = 이 순서는 전체 화면으로 못 박기.
- 출력 데이터에 `audio`(이 출력이 소리 내는 출력인가) 덧붙임. 옛 output.js 는 이 칸을 모르고 예배당 창에서만 소리 → 같은 결과.
- 옛 2.0.x: 새 키를 모른 채 그대로 둔다 (deep_merge_defaults · 순서는 JSON 그대로 저장). `/outputs/out-…` 는 옛 서버에서 404. 없는 구성 · 지운 출력을 가리키는 지정은 건너뛴다(오류 아님).
- 따라가는 곳: **큐시트 파일**(내보내기 · 가져오기 — 순서 JSON 그대로, e2e 가 `--import-cue` 로 순서 `scenes` 를 가져와 씀) · **여러 PC 동기화**(큐시트 순서와 함께 `scenes` 가 감. 구성 자체(`scenes` 설정)는 템플릿처럼 동기화하지 않으므로 다른 PC 에 같은 구성이 없으면 '따라감' 으로) · **다른 컴퓨터로 옮기기**(settings.json 통째로 → 새 키 모두).
- 구성을 지우면 `outputScenes` · `typeScenes` 의 그 지정도 지운다. 추가 출력을 지우면 그 출력의 `outputScenes` · `typeScenes` · `audioOutput` 정리.

### 규칙 · 바꾼 곳

| 곳 | 바꾼 것 |
|---|---|
| `wp-core/src/outputs.rs` (새) | 출력 목록 · 추가 출력 정리/저장/지우기 · `kind_of` · 소리 출력 · `resolve_scene`(순서 → 종류 → 출력 → full) · `item_differs`(표시) · 시험 4개 |
| `core.rs` | `template_for_deck`: 추가 출력은 자기 템플릿 → 같은 종류 내장 출력 규칙. `output_payload`: 종류(kind)로 obsFull · 그림 우상단 · 배경 미디어, 'OBS 화면 숨김'(예배당을 끄면 같이)은 예배당 · 방송만, `audio` 덧붙임. 순서 메타에 `scenes` |
| `sources.rs` | `scene_id_for_deck`/`scene_choice_for_deck`(송출 중 순서가 열린 큐시트 순서면 지금 값을 봄), `stage_template_for_deck`, 출력 · 소리 · 순서/종류 지정 명령, 시험 2개(시험 기준 · 추가 출력) |
| `trackb.rs overlays_for` | 추가 출력은 같은 종류 내장 출력의 오버레이 · 회중 메시지 |
| `server.rs` · `lan.rs` | `/outputs/out-…` → 같은 index.html, SSE `client=out-…`(설정에 있을 때), `camTestReport` 명령(조작 서버만) |
| `camtest.rs` (새) · `camhelper.rs` | `/helper/camtest` 페이지 · 일회용 토큰 · 결과 줄 |
| `web/output/output.js` | 출력 이름 = 주소 끝(`out-…` 모양만, 나머지는 예전처럼 main), 소리 = `payload.audio`(없으면 예전 규칙) |
| wp-app | `sources_ui.rs` 화면 구성 구획 · `scene_marks`, `outputs_ui.rs`(새: 출력 구획 · 추가 출력 창 관리 — 내가 띄운 PID 만 끔, `--shot` 점검 때는 안 띄움), `camtest_ui.rs`(새), `app.rs` 뷰어 출력 고르기 · Snap `extra_views` · `scene_marks`, `cue_ui.rs` · `timeline_ui.rs` 표시, `scene_ui.rs` 출력 목록 · Alt+누름(`cycle_pick`), `trackd_ui.rs` 출력 창 스크롤, `menubar.rs` 연결 메뉴, `settings_items.rs` 찾기 항목 |

### 확인 (2026-11-27)

- 시험: wp-core 334 통과(19 제외, 새 7: outputs 4 · sources 2 · camtest 1, lan 규칙 덧붙임) · wp-app 241 통과(1 제외, 새: Alt+누름 계산 · headless Alt+누름 고르기 · 끌기, 추가 출력 창 인자, 시험 주소). `auditc_tests::custom_shortcuts_for_quick_text_are_respected` 는 병렬로 한 번 임시 DB 열기에서 흔들림 — 혼자 · 다시 돌리면 통과.
- **아무것도 안 바꾼 사용자**: `tools/scenes/e2e.js --old-exe <2.0.8 web>` — `/outputs/main` · `/outputs/overlay` 가 2.0.8 output.js 와 **다른 화소 0개**. 2단계 시험 기준 · 좌우 나눔도 그대로 통과.
- parity (`tools\parity\run.ps1` + `verify.js`, 50 경우): 46/48 같은 글자 크기 — 다른 둘은 2.0.7 부터의 obs-pip-image · obs-pip-score (2단계와 같은 결과). 슬라이드 그리는 코드는 안 바꿈.
- **시험 기준** `node tools/scenes/e2e-outputs.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\outputs-e2e --fake-cam` (9개 모두 통과):
  큐시트 파일(성경봉독 순서에 `scenes: { main: 본문 전체, overlay: 카메라 전체 + 아래 성경 자막 }`)을 가져와 송출, 찬양 종류 기본 `{ main: 가사 전체, overlay: 가사 전체 + 구석 작은 카메라 }`, 추가 출력 '로비 화면'.
  ① 성경봉독: 예배당 칸 background · live / 방송 칸 camera · live (카메라 1920×1080 들어옴, 아래 띠 자막) / 로비 = 예배당 기본 구성.
  ② `/api/trigger/cue-next`(외부 단추와 같은 길)로 찬양: 예배당 background · live / 방송 background(예배당 템플릿) · live · 구석 카메라(둥글기 16 · 흰 테두리) — **같은 페이지(다시 열지 않음)가 SSE 로 각자 바뀜**.
  캡처: `tmp\outputs-e2e\1-bible-{main,obs,lobby}.png` · `2-song-{main,obs,lobby}.png`. 카메라는 브라우저 시험 카메라(--fake-cam), 이 PC 의 OBS 가상 카메라는 쓰지 않음.
- 조작 화면 캡처(`--shot`): `tmp\ui-shots\inspector.png`(소스 탭 화면 구성 · 큐시트 표시 · 뷰어 [예배당 | OBS | 로비 화면]) · `output.png`(출력 설정) · `scene.png`(편집기 쓰는 곳 셋) · `camtest.png`.
- **확인 못 한 것**: 추가 출력을 실제 둘째 모니터에 띄우기(이 PC 는 모니터 하나 — 창 인자 시험만), 카메라 동시 열기 시험의 실제 웹캠 결과(페이지 JS 문법 · 결과 줄 시험만, 교회 PC 에서), 추가 출력 창에서 영상 소리, 맥 빌드(맥 전용 코드는 `open` 명령으로 브라우저 열기 한 줄 — CI 로 확인 필요).

### 다른 갈래와 겹칠 곳 (ai-features)

- `crates/wp-core/data/en.json` 맨 앞 65줄 · `core.rs`(template_for_deck 머리 · output_payload · fill_item_meta) · `server.rs`(events · command · output_routes) · `trackb.rs overlays_for`.
- `app.rs`(Snap 필드 둘 · 뷰어 머리 · MonitorViews) · `cue_ui.rs` RowState `scene` · `timeline_ui.rs` 막대 툴팁 · `menubar.rs` 연결 메뉴 한 줄 · `trackd_ui.rs`(handle_menu · frame · 출력 창 높이/스크롤 · output_ui 끝) · `settings_items.rs` 한 줄 · `main.rs` mod 두 줄.

### 4단계(전환 · 자동화) 전에 정할 것

1. **구성이 바뀔 때 전환**: 지금은 칸이 바로 바뀐다(본문만 디졸브). 구성 단위 디졸브 / 컷 / 같은 소스는 그 자리에서 옮겨 가기(카메라 끊김 없음) 중 무엇을 기본으로.
2. **'송출할 때 자동으로'에 구성 바꾸기**를 따로 둘지 — 3단계의 순서 지정은 '그 순서 동안만'. 바꾼 채로 남는 동작(출력 기본을 바꿈)이 필요한지.
3. 매크로 · 예약 · `/api/trigger/scene?output=…&id=…`(스트림덱) 로 구성 바꾸기 — 바꾸는 대상이 출력 기본인지, 지금 한 번(다음 순서까지)인지.
4. 추가 출력도 '예배당 화면 켜기 · 끄기'(Ctrl+Shift+F)와 같이 켜고 끌지 (지금은 따로).
5. 무대 화면을 구성으로 (이번에 그대로 둔 ③) · 소리를 출력 여러 곳에서 낼지(지금 하나).
6. 카메라 동시 열기 시험 결과에 따라 '한 곳에서 열어 나눠 주기'(바 대책)를 4단계 전에 넣을지.

## 4단계 한 것 (2026-11-27, sources 가지, 아직 릴리스 전)

주인 결정(3단계 '정할 것' → 작업 지시): ① 기본 전환 = 같은 소스는 그 자리에서 새 자리로 옮겨 가기, 새로 생기거나 사라지는 칸은 서서히 · 고를 수 있는 것 바로 / 서서히 / 옮겨 가기, 기본 0.4초 ② '송출할 때 자동으로' 에 '구성 바꾸기' = 출력 기본을 바꾸는 남는 동작 ③ 외부 단추 `/api/trigger/scene` 은 기본 '지금 한 번(다음 순서까지)', `hold=1` 이면 출력 기본 ④ 추가 출력마다 '예배당 화면과 함께 켜고 끄기'(기본 꺼짐) ⑤ 예배 시간표에 구성 줄(보기 전용) ⑥ 무대 화면 구성 · 여러 출력 소리 · 카메라 나눠 주기는 하지 않음(아래 제안).

### 쓰는 법

1. **전환 고르기**: 화면 → 출력 설정 → '출력' 구획 **화면 구성이 바뀔 때** [옮겨 가기 ▾] [0.4초]. '출력마다 다르게' 를 펴면 출력마다 [전체 기본 따라감 / 옮겨 가기 / 서서히 / 바로 (컷)].
   구성마다: 화면 구성 편집기 가운데 '미리 보기' 줄 오른쪽 **이 구성으로 바뀔 때** [출력 설정 따라감 ▾].
   고르는 차례: 바뀌어 들어가는 구성 → 출력 → 전체 기본 → 옮겨 가기 0.4초.
   - **옮겨 가기**: 카메라 · 웹 페이지 칸은 같은 요소(video · iframe) 그대로 새 자리 · 크기로 미끄러진다 (끊김 없음 · 다시 열지 않음). 송출 중인 순서 · 배경 같은 글 칸은 자리가 바뀌면 새 크기로 다시 맞춰 서서히. 새로 생기거나 사라지는 칸은 서서히.
   - **서서히**: 바뀐 칸은 모두 서서히 겹쳐 바뀜 (카메라도 같은 스트림으로 — 다시 열지 않음).
   - **바로 (컷)**: 바로 바뀜 (그래도 같은 카메라 요소는 그대로 씀).
   - 처음 그릴 때 · 같은 구성 안에서 칸을 고칠 때(편집기)는 전환 없이 바로.
2. **송출할 때 자동으로 → 화면 구성**: 순서 편집칸 '송출할 때 자동으로' 에 '화면 구성' 줄(구성이 하나라도 있을 때). 출력마다 [그대로 / 전체 화면 (기본) / 구성…].
   그 순서를 송출하면 **출력 기본**(출력 설정 · 편집기 '쓰는 곳' 과 같은 값)을 바꾸고 다음 순서에서도 남는다. 이 순서 동안만 바꾸려면 3단계의 인스펙터 '소스' 탭(순서 지정 — 그쪽이 먼저).
   매크로로 저장(편집칸 '매크로로 저장')하면 매크로 · 예약 실행(30초마다)도 같은 동작을 한다.
3. **외부 단추 · 스트림덱 · Companion**: `GET /api/trigger/scene?key=…&output=overlay&id=<구성 id 또는 이름>`
   - 기본 = **지금 한 번**: 송출 중인 순서 동안만, 다음 순서로 넘어가면(다른 순서를 송출하면) 원래대로. 같은 순서를 다시 보내면 그대로.
   - `&hold=1` = 출력 기본을 바꿈 (남음). `output=all` = 모든 출력. `id=full` = 전체 화면 (기본). `id` 를 비우거나 `&clear=1` = 지금 한 번 지정 풀기.
   - 연결 → 스트림덱 · 외부 단추 의 동작 목록에 'scene' 이 보인다. 고르는 차례: **지금 한 번 → 순서 지정 → 종류 기본 → 출력 기본 → 전체**.
4. **휴대폰 리모컨 · OBS 도킹**: 구성이 하나라도 있으면 '화면 구성' 카드 — 출력마다 [전체 화면 (기본) | 구성…] 단추. 누르면 지금 한 번, 오른쪽 위 '계속 쓰기' 를 켜고 누르면 출력 기본. 지금 쓰는 구성은 파란 테두리, 출력 기본은 점선, 지금 한 번이면 '— 다음 순서까지' 와 [되돌리기].
5. **추가 출력 켜고 끄기**: 출력 설정 → 추가 출력마다 **예배당 화면과 함께 켜고 끄기** (모니터를 고른 출력만, 기본 꺼짐). 예배당 화면(Ctrl+Shift+F · 컨트롤 막대 · 송출할 때 자동으로 켜기)이 켜지는 순간 그 모니터 창도 띄우고, 꺼지는 순간 내린다. 그 사이 손으로 바꾼 것은 그대로.
6. **예배 시간표 구성 줄**: 구성이 하나라도 있으면 시간표 아래에 '구성' 줄. 출력 기본과 다른 구성을 쓰는 순서는 막대(구성 이름), '송출할 때 자동으로: 화면 구성' 이 있는 순서는 막대 왼쪽 끝 세로 금. 마우스를 올리면 '이 순서 동안: 방송 (OBS): 카메라 + 자막' · '송출할 때 기본을 바꿈: …'. 보기 전용.

### 저장 형식 (덧붙이기만)

- 설정 `sceneTransition`: `{ type: "move"|"fade"|"cut", ms }` (없으면 move 400). 설정 `outputTransitions`: `{ <출력 id>: {type, ms} }`. 구성 `scenes[].transition`: `{type, ms}` (없으면 따라감). ms 0~3000, cut 은 0.
- 순서 `actions.scene`: `{ <출력 id>: <구성 id | "full"> }` (actions 정리 규칙에 덧붙임 — 출력 · 구성 id 모양만 보고, 없는 것은 실행 때 건너뛰고 알림).
- 추가 출력 `extraOutputs[].followMain: true` (꺼져 있으면 칸 자체가 없음 — 옛 모양 그대로).
- '지금 한 번' 은 저장하지 않는다 (코어 `b.scene_once`: 출력 → 구성, 고를 때 송출 중이던 순서 열쇠).
- 출력 데이터에 `sceneTransition: {type, ms}` 덧붙임. 옛 output.js 는 모른 채 바로 바꾼다. 웹 출력 `<body data-scene="…">`(점검용, 그림에 안 씀).
- 옛 2.0.x: 새 키를 모른 채 그대로 둔다. 추가 출력을 지우면 그 출력의 `outputTransitions` 도 지운다.

### 규칙 · 바꾼 곳

| 곳 | 바꾼 것 |
|---|---|
| `wp-core/src/scenectl.rs` (새) | 전환 정리 · 고르는 차례(`resolve`) · 저장(`set`, 대상 "" · output: · scene:) · 순서 동작 정리/요약 · '지금 한 번'(`SceneOnce`, `trigger_scene` · `clear_scene_once` · `scene_once_after_send`) · `run_scene_action` · 시험 3개 |
| `sources.rs scene_choice_for_deck` | 송출 중인 묶음이면 '지금 한 번' 이 맨 먼저 (`outputs::From::Once`) — 미리보기 묶음에는 안 붙음 |
| `core.rs` | `output_payload` 에 `sceneTransition`, `send_deck` 이 다른 순서로 넘어가면 '지금 한 번' 지움 |
| `actions.rs` · `trackd.rs run_item_actions` | `actions.scene` 정리 · 요약('구성: 방송 (OBS) → …') · 실행 |
| `trigger.rs` | 동작 `scene` (output · id · hold · clear) |
| `remote.rs` · `web/remote` | 상태 `scenes` {outputs:[{id,name,now,once,base}], list}, 조작 `scene` {output,id,hold,clear}, 카드 · '계속 쓰기' |
| `outputs.rs` | `followMain` 정리, 지울 때 전환 지정도 지움 |
| `scenes.rs normalize_scene` | 구성의 `transition` 정리 (맞지 않으면 지움) |
| `web/output/output.js` | 칸 짝 맞추기(같은 칸 id+소스 → 같은 소스), 카메라 · 웹 칸은 늘 그릴 자리(host)를 두고 자리 · 자르기가 바뀌면 `render.js sceneHostFit` 로 그 자리만 고침(같은 요소), 템플릿이 바뀌어 무대를 새로 만들 때도 카메라 · 웹 칸은 새 무대로 옮김, 전환: `glide`(FLIP — 새 자리에 놓고 transform 으로 예전 자리에서 시작, Web Animations), `enter`/`leave`(투명도), 서서히 때 예전 자리 카메라 잔상(`camGhost`, 같은 스트림) |
| `web/shared/render.js` | `sceneHost` 를 `sceneHostFit` 으로 나눔(그림 결과 같음), `sceneRect` 내보냄 |
| wp-app | `outputs_ui.rs` 전환 줄 · 출력마다 · '예배당 화면과 함께 켜고 끄기'(`follow_changes`, 예배당 창이 켜지고 꺼지는 순간에만) · `transition_pick`, `scene_ui.rs` '이 구성으로 바뀔 때', `actions_ui.rs` '화면 구성' 줄(`set_scene_action`), `timeline_ui.rs` 구성 줄, `app.rs` Snap `scene_acts` |

### 확인 (2026-11-27)

- **시험 기준** `node tools/scenes/e2e-auto.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\auto-e2e --fake-cam` (20개 모두 통과):
  ① 성경봉독(방송 = 카메라 전체 + 자막) → `/api/trigger/cue-next` → 찬양(방송 = 가사 + 구석 카메라): 방송 출력의 카메라 칸이 **같은 video 요소**(표시해 둔 요소 그대로) · 147프레임 동안 멈추지 않고 영상 시간이 흐름 · transform 애니메이션 412ms · 1920px → 518px 구석.
  ② `/api/trigger/scene?output=overlay&id=sc-camsub` → 카메라 전체(같은 video 요소), 구성 이름으로도 고름, 다음 순서로 넘기면 기본(전체)으로 되돌아옴.
  ③ `hold=1` → 다음 순서에서도 남음 · 설정 `outputScenes.overlay` 저장. 순서 동작 '구성 바꾸기'(광고 하나: 예배당 → 가사 전체)도 다음 순서에 남음. 페이지는 다시 열리지 않음(SSE).
  ④ 리모컨 상태 · 도킹 화면에 출력마다 구성 단추.
  캡처: `tmp\auto-e2e\1-bible-obs.png` · `2-moving-obs.png`(옮겨 가는 중) · `3-song-obs.png` · `4-once-obs.png` · `5-next-obs.png` · `6-hold-{obs,main}.png` · `7-remote-dock.png`, 기록 `report.json`(프레임마다 상자 폭 · 애니메이션 · 같은 요소).
- **아무것도 안 바꾼 사용자**: `tools/scenes/e2e.js --old-exe <2.0.8 web>` — `/outputs/main` · `/outputs/overlay` 다른 화소 0개. 3단계 `e2e-outputs.js` 9개 그대로 통과.
- 시험: wp-core 337 통과(19 제외, 새 3: scenectl) · wp-app 243 통과(1 제외, 새 2: 순서 동작 구성 · 예배당 따라 켜고 끄기).
- 조작 화면 캡처(`--shot`, 자료 `tmp\ui4\data`): `tmp\ui4\shots\timeline.png`(시간표 구성 줄 · 기본 바꾸는 금) · `output.png`(출력 설정: 화면 구성이 바뀔 때 · 출력마다 다르게 · 예배당 화면과 함께 켜고 끄기, `--open-d output`) · `scene.png`(편집기 '이 구성으로 바뀔 때', `--scene-open`).
- 코어 시험 `scenectl::tests::once_hold_and_item_action`: 지금 한 번(이름으로) → 같은 순서 다시 보내도 그대로 → 다음 순서에서 되돌아옴(저장 안 함) → hold 남음 → 순서 동작(모르는 출력 건너뜀) → all · 풀기 · 없는 구성/출력 오류 → 전환 차례.
- **확인 못 한 것**: 실제 둘째 모니터에서 '예배당 화면과 함께 켜고 끄기'(이 PC 는 모니터 하나 — 순간 감지 시험만), 실제 스트림덱 · Companion 단추(주소 GET 은 같은 길), OBS 브라우저 소스(CEF) 안의 Web Animations(크롬 84 이상이면 됨 — OBS 27 이상), 실제 웹캠.

### 하지 않은 것 · 제안

- **무대 화면을 구성으로**: stage.html 은 그대로. 무대도 출력 하나로 보고 구성을 고르게 하려면 무대용 소스(다음 슬라이드 · 타이머 · 메모 · 고정)를 소스로 꺼내야 한다 → 5단계 후보.
- **여러 출력에서 소리**: 지금처럼 하나. 둘 이상이면 같은 영상의 소리가 겹치고(지연 차이로 메아리) 장치마다 고르기가 필요 → 출력마다 '소리 장치(setSinkId)' 를 고르는 방식이 안전.
- **카메라 한 곳에서 열어 나눠 주기**: 교회 PC 의 '카메라 동시 열기 시험'(연결 메뉴) 결과를 기다림. 같은 카메라를 예배당 창과 OBS 가 각자 열지 못하면, 예배당 창(wp-output)이 열고 WebRTC 로 OBS 브라우저 소스에 넘기는 길(휴대폰 카메라의 phonecam 길을 다시 씀)이 가장 짧다.
- 전환을 구성 사이 '쌍' 마다 다르게(가 → 나 는 서서히, 나 → 가 는 컷)는 하지 않음 — 들어가는 구성 하나에 붙임.
