# 함께 쓰는 공개 소프트웨어 · 자료 (Third-party notices)

워십프리젠터(Worship Presenter)의 소스 코드는 **GNU GPL v3 또는 그 이후 판**(`GPL-3.0-or-later`)으로 공개합니다 — [LICENSE](LICENSE).
아래 코드·글꼴·자료는 각자의 라이선스를 따릅니다. GPL 은 이것들의 라이선스를 바꾸지 않습니다.

## 프로그램에 들어 있는 코드 · 글꼴

| 이름 | 라이선스 | 쓰는 곳 |
|---|---|---|
| [dockview-core](https://github.com/mathuo/dockview) 8.4.0 | MIT | 조작 화면 패널 배치 (`src/web/shared/vendor/dockview-core.min.js`) |
| [qrcode-generator](https://github.com/kazuhikoarase/qrcode-generator) 2.0.4 | MIT | 휴대폰 리모컨 QR 코드 (`src/web/shared/vendor/qrcode-generator.js`) |
| [Lucide](https://lucide.dev) 아이콘 | ISC | 조작 화면 아이콘 (`src/web/shared/icons.js` 에 그림 경로를 옮김) |
| [Pretendard](https://github.com/orioncactus/pretendard) | SIL Open Font License 1.1 | 조작 화면 글꼴 (`src/web/shared/fonts/`, 라이선스 `OFL-Pretendard.txt`) |
| [Electron](https://www.electronjs.org) | MIT | 실행 환경 (설치판에 함께 들어감, `LICENSE.electron.txt` · `LICENSES.chromium.html`) |
| [이음오피스 IeumOffice](https://github.com/pblsketch/ieumoffice) PPTX 엔진 (pptx-engine · pptx-render · font-metrics · zip-gate) | Apache License 2.0 | PPT(.pptx/.ppsx) 읽기 · 글꼴 메트릭으로 글 배치 · 차트·표·SmartArt 그리기 (`src/core/vendor/ieum-pptx.js` — **고쳐서 묶은 것**: 읽기·그리기 부분만 esbuild 로 CommonJS 하나로, `@genoffice/agent-core` 는 상수 3개 stub 으로 바꿈. 원본 커밋은 파일 머리말에. 라이선스 전문 `src/core/vendor/LICENSE-ieumoffice.txt` · NOTICE `src/core/vendor/NOTICE-ieumoffice.txt`) · `src/core/pptxdraw.js` 는 이음오피스 `apps/slides/src/renderer/export-svg.ts` · `konva-adapter.ts` 를 Node 용으로 옮겨 고친 것 |
| [jszip](https://github.com/Stuk/jszip) 3.10.1 | MIT (또는 GPL-3.0-or-later 중 택일) | 위 묶음 안 — .pptx(zip) 풀기 |
| [fast-xml-parser](https://github.com/NaturalIntelligence/fast-xml-parser) 5.10.1 | MIT | 위 묶음 안 — OOXML 읽기 |
| [opentype.js](https://github.com/opentypejs/opentype.js) 2.0.0 | MIT | 위 묶음 안 — 글꼴 파일 읽기 (글자 너비·올림·내림) |
| [bidi-js](https://github.com/lojjic/bidi-js) 1.0.3 | MIT | 위 묶음 안 — 양방향(아랍어·히브리어) 글 순서 |

묶음 안 npm 패키지의 라이선스 전문은 `src/core/vendor/LICENSES-ieum-pptx-deps.txt` 에 있다.

### 이음오피스 NOTICE (Apache-2.0 §4(d) 에 따라 옮김 — 전문은 `src/core/vendor/NOTICE-ieumoffice.txt`)

> GenOffice — Copyright 2026 Mainfunc, Inc. This product includes software developed at Mainfunc, Inc.
> IeumOffice (https://github.com/pblsketch/ieumoffice) is a modified version of GenOffice (https://github.com/genspark-ai/genoffice). The modifications are licensed under the Apache License, Version 2.0, like the original work. IeumOffice is not affiliated with, sponsored by or endorsed by Mainfunc, Inc.

워십프리젠터는 그 가운데 PPTX 엔진 부분만 가져와 고쳐 묶었다 (`tools/build-ieum-pptx.js`). 이음오피스·GenOffice 의 다른 부분(글꼴 · 디자인 템플릿 · 스킬 등)은 들어 있지 않다.

## 그 밖의 자료

- **성경 본문**: 개역한글은 대한성서공회 안내상 저작재산권 보호기간이 만료된 번역본입니다. 다른 번역본·찬송가·악보·가사는 들어 있지 않으며 각 권리자의 조건을 따릅니다.
- **구세군 레드 쉴드·크레스트 로고**: 구세군(The Salvation Army)의 상표입니다. GPL 로 공개하는 코드에 들어가지 않으며, 구세군 교회·기관의 예배 송출용으로만 포함했습니다.
