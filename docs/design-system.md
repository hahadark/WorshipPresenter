# 조작 화면 디자인 체계

2026-11-28 · `craft-main` · F1 소유: `theme.rs`, `widgets.rs`, `kit.rs`, `icons.rs`.
기존 전문가용 고정 판 배치는 유지한다. 부품은 `use crate::widgets;` 하나로 쓴다. 창 내부는 F2가 같은 API를 사용한다.

## 토큰

| 이름 | 크기(px) | 굵기 | 줄 높이(px) | 용도 |
|---|---:|---:|---:|---|
| `Type::Micro` | 10.5 | 500 | 14 | 시계 이름표, 눈금, 단축키 |
| `Type::Small` / `SmallStrong` | 11.5 | 400 / 600 | 16 | 양식 이름표, 부가 글 / 구획 머리 |
| `Type::Body` / `BodyStrong` | 12.5 | 400 / 600 | 18 | 조절 칸, 표 / 판 머리, 고른 줄 |
| `Type::Title` / `Read` | 14 | 600 / 400 | 20 / 22 | 순서 제목 / 읽는 본문 |
| `Type::Display` | 20 | 600 | 24 | 고정 폭 시각 |

Pretendard만 사용한다. `theme::ty(text, Type, color)` 또는 `Type::… .font()`를 쓴다. 출력용 글꼴과 슬라이드 그리기에는 이 규칙을 적용하지 않는다.

간격은 `theme::sp::{X4,X8,X12,X16,X20,X24,X32}`의 4px 격자. `X2`, `X6`은 아이콘 내부·글과 아이콘·촘촘한 줄 보정에만 사용한다. 판 좌우 여백 `sp::PANE=12`. 단추/입력칸은 `ctl(Ctl::Sm/Md/Lg)` = 22 / 26 / 32. 판 머리 `HEAD_H=28`, 목록 줄 `ROW_H=28`, 구획 머리 `SECTION_H=24`, 상태 줄 `STATUS_H=24`, 위 막대 `BAR_H=56`.

판 둥글기 0. 조절 칸은 `cr(Rad::Md)` = 4, 그 안에 1px 들인 선택칸 `Rad::Sm` = 3. 메뉴·툴팁·창만 6. 선 `LINE=1`, 초점과 끼움선 `LINE_EMPH=2`, 송출 뷰어 `LINE_LIVE=3`. 임의 그림자와 카드 장식은 추가하지 않는다.

## 색과 상태

`pal()`의 역할을 쓴다. `bg` 전체 바탕 → `panel` 판 → `head` 머리 → `raised` 조절 칸, `sunken` 입력과 트랙. 글은 `text` / `text2` / `muted` / `muted2`, 선은 `line` / `line2` / `ctl_line` / `edge`.

빨강 `live` / `live_dk` / `live_text`는 송출 중에만, 노랑 `next` / `next_bg`는 다음에만 쓴다. 고른 조절 칸은 `sel`과 밝은 무채색 선, 초점은 `focus`, 오류는 `danger_text` 주황, 성공은 `text2` 회색. `Kind::Danger`는 채우지 않은 보통 선 단추이며 확인·되돌리기를 함께 둔다.

| 상태 | 값 |
|---|---|
| 보통 | 역할 바탕 + 글 + 1px 선 |
| 올림 | `lift(base)` (어두운 테마 흰색 혼합, 밝은 테마 검정 혼합) |
| 눌림 | `press(base)` (어두운 쪽으로 14~16%) + 안쪽 윗선 |
| 초점 | `focus`, 2px 고리, 1px 간격. 색만 바꿔 초점을 숨기지 않음 |
| 꺼짐 | `disabled(color, panel)` = 글 38% + 판 62%; 상호작용 없음 |
| 고름 | `sel`, `text`, 무채색 선. 송출/다음 상태가 우선 |

`Vis::{NORMAL,HOVER,PRESS,FOCUS,OFF,ON}`으로 강제 상태를 그릴 수 있다. 실제 부품은 `Vis::of`로 읽는다. 표는 `RowVis`와 `paint_row`: 송출·다음은 3px 왼쪽 선과 낮은 채도 바탕, 고름은 회색, 지나감은 흐린 글. 긴 글은 `elide` / `text_elided`와 전문 툴팁. 숫자는 `tnum` / `Tnum`으로 자릿수 고정, 표에서는 오른쪽 맞춤.

## 부품 API

| 부품 | API |
|---|---|
| 단추 / 아이콘 / 펼침 | `Btn` + `Kind::{Primary,Default,Ghost,Danger,Send}`, `.icon_only`, `.caret` |
| 구획 단추 / 탭 | `segmented[_with/_cells]`, `tabs` |
| 고르기 칸 | `popup_btn`, `select` |
| 체크 / 라디오 / 스위치 | `checkbox[_enabled]`, `radio`, `switch` |
| 슬라이더 | `slider` (초점이 있으면 좌우 화살표) |
| 글 입력칸 | `text_field[_sized]` (egui TextEdit의 선택·IME 유지) |
| 표 / 목록 | `paint_row` + `RowVis`; 클릭·끌기 정책은 사용하는 화면 소유 |
| 구획 머리 | `section_head`, `group_head` |
| 양식 | `form_row`, `form_label`; 이름표 열 너비 통일 |
| 툴팁 | `tip(response, 설명, 단축키)`, `tip_auto` |
| 오른쪽 클릭 메뉴 | egui `response.context_menu`; 전역 테마의 글·간격·둥글기 적용 |
| 스크롤 막대 | egui `ScrollArea`; 전역 테마의 선·트랙 사용 |
| 판 사이 손잡이 | `splitter` |
| 알림 띠 | `notice` + `Tone::{Info,Ok,Warn,Error}` |
| 빈 상태 / 진행 | `empty`, `loading`, `progress` |

모든 조작 가능한 새 부품은 `Sense::click` 또는 TextEdit로 Tab 초점을 받는다. Enter/Space는 egui 클릭 동작, 슬라이더 좌우 키는 한 단계 이동. 툴팁은 단축키를 별도 칩으로 표시한다.

## 움직임과 다시 그리기

`T_FAST=0.09s` 올림·눌림, `T_MED=0.13s` 고름 이동·접기. `anim_bool` / `anim_value`는 egui가 움직이는 동안만 프레임을 요청한다. `ui.reduceMotion` 기본 꺼짐, 설정 → 조작 화면 → 움직임 줄이기. 켜면 즉시 상태를 바꾸며 출력 화면의 전환은 그대로다. 불확정 진행/돌림 표시만 보이는 동안 33/50ms에 요청하고, 감소 설정에서는 정지한다.

## 견본과 캡처

`wp-app --specimen --win 1440x1000 --shot tmp/craft/specimen.png`는 실제 자료, DB, 출력 서버, 모니터를 열지 않는다. `--data`를 함께 주면 그 폴더의 `settings.json`에서 테마와 배율만 읽는다. `--specimen icons`는 이름표와 16/20/24px 아이콘을 나열한다. 견본의 진행 시각은 고정한다.

`node tools/craft/shots.js --exe target/debug/wp-app.exe --seed tmp/craft/seed --out tmp/craft/after --only specimen`로 테마와 배율별 그림을 만든다. 앱이 직접 찍고 종료하며 키보드·마우스를 건드리지 않는다. 실제 주 화면의 전후 쌍은 `tmp/craft/before`, `tmp/craft/after`에 보관한다.

견본 그림은 검증 후 이 문서에 추가한다. 아이콘 원본·이름표는 다음 커밋에서 추가한다.
