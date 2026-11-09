// 이음오피스(IeumOffice) PPTX 엔진 묶음의 들머리 — tools/build-ieum-pptx.js 가 esbuild 로 이 파일을 src/core/vendor/ieum-pptx.js 로 묶는다.
// 여기서 내보내는 것만 워십프리젠터가 쓴다 (읽기·그리기만 — 저장·편집 기능은 묶이지 않는다).
//   @ieum/… 는 build-ieum-pptx.js 의 플러그인이 이음오피스 소스 폴더로 바꿔 준다.

// .pptx 읽기 (zip → 슬라이드 모델, 레이아웃·마스터·테마 상속 풀기)
export { openPptx } from '@ieum/pptx-engine/src/index.ts'
export { elementSpid } from '@ieum/pptx-engine/src/animation.ts'
// 슬라이드 모델 → 그릴 준비가 끝난 데이터(RenderTree): 픽셀 좌표 · 채우기·선 · 글꼴 메트릭으로 줄바꿈한 글
export {
  buildRenderSlide,
  HeuristicMetrics,
  OpentypeMetrics,
  graphemes,
  isWideChar,
  layoutText,
  imageSizeFromBytes,
  imageDpiFromBytes,
  EMU_PER_PT,
  EMU_PER_PX_96,
  emuToPx,
  ptToPx,
  makeViewport,
  presetPath,
  presetPolygon,
  isConnectorPreset,
} from '@ieum/pptx-render/src/index.ts'
// 설치된 글꼴 찾기 (글꼴 폴더 훑기 · 이름표 읽기 · 굵게/기울임 고르기 — 파일 전체를 읽지 않고 필요한 표만)
export { getFontIndex, styleScore, styleTokens, norm as fontNameKey, readTableDir, readTable, faceOffsets } from '@ieum/font-metrics/src/sfnt.ts'
export { findSystemFont, isFamilyInstalled } from '@ieum/font-metrics/src/font-locate.ts'
// 글꼴 파일 읽기 (글자 너비·올림·내림)
export { parse as parseFont } from 'opentype.js'
