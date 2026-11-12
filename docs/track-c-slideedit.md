# 트랙 C 슬라이드 편집기 보완 (GPT 작업)

## 비교 근거

읽기 전용 Electron 원본: src/core/slideedit.js, src/web/control/slideedit.js, src/web/shared/slides.js.
기존 Rust의 정규화(crop/font/opacity), 편집 병합, 일반 제목 카드 추가/삭제는 재사용했다.

## 구현

- 제목 장: more를 포함한 추가·삭제·복제·상하 이동, 제목/부제/나머지 장으로 다시 저장. 마지막 제목 장 삭제 방지.
- 슬라이드 목록 드래그: 찬양/공지의 원래 글 덩어리, 제목 장, 원본 슬라이드와 덧붙인 카드 정렬. 숫자 편집 키도 재배치.
- Electron 제한 유지: 성경 드래그와 부를 순서(order)를 지정한 찬양의 드래그는 변경하지 않음.
- 덧붙인 카드 본문 편집: extra[].text 갱신. 삭제 시 해당 x키 편집 제거.
- 요소: font, opacity, image crop 입력. crop은 Electron 범위와 좌우/상하 합 제한 적용.
- 미리보기 요소 드래그: 화면 백분율 좌표로 이동, 다른 속성 보존. 공통 요소는 공통 모드에서 이동.
- 그림 미리보기: crop UV와 cover/contain, opacity 반영. 기존 글꼴 렌더 재사용.
- 찬송가/PPT: 별도 생성 경로를 공통 decorate에 연결하여 slideOrder/extra 적용. PPT 배경 누락 시 추출 요소 중복 방지 유지.

## 형식

Electron 호환 title/subtitle/more, extra[{id,at,text}], slideOrder, edits[숫자 또는 x아이디 또는 *]를 유지한다.
main.rs, lib.rs, app.rs, 출력 웹 코드는 변경하지 않았다.

## 검증

- cargo test -p wp-core -p wp-app --offline: wp-app 7 통과, wp-core 105 통과 / 13 제외, 문서 테스트 0. 실패 0.
- cargo build -p wp-app --offline: 개발 빌드 성공. 기존 경고는 남아 있음.
- 로그: tmp/slideedit-test.log, tmp/slideedit-build.log.
- 추가 단위 테스트: 제목 장 왕복/마지막 장, 편집 키 동반 정렬, 덧붙임 at 재계산 및 본문, 요소 속성/좌표, 제목 드래그 및 PPT 누락 배경 회귀.

## 미검증

실제 Windows 창에서 마우스로 조작하는 수동 GUI 검증은 수행하지 않았다. 화면 스크린샷 없음.
사용자 자료/DB와 앱 프로세스는 변경하거나 종료하지 않았다. 앱 실행도 하지 않았다.
릴리스 빌드, 푸시, 외부 API 호출, 다른 모델 호출 없음.
