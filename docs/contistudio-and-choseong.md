# 찬양 콘티 스튜디오 콘티 가져오기 · 초성 검색 (2026-11-25)

찬양 콘티 스튜디오 CL(윤명중, AGPL-3.0, v5.5.0)에서 쓸 만한 기능을 골라 옮겼다. 코드는 옮기지 않고 파일 형식과 동작만 보고 새로 짰다.

## 1. 콘티(.worship) 가져오기

- 들어가는 곳: `큐시트 → 찬양 콘티 가져오기 (콘티 스튜디오 .worship)…`(가져온 큐시트를 바로 연다), `큐시트 관리 → 가져오기…`(.json 과 .worship 을 함께 고를 수 있음).
- 코어: `wp-core/src/contistudio.rs` (`is_conti` · `form_to_order` · `to_cue`), `Core::import_cue_files_ex` (cuemgr.rs). 점검 실행: `wp-app --data <임시> --import-cue <파일> --item 0 --shot a.png`.
- .worship 은 JSON 이다: `service_info`(date · service_name · worship_leader · team_members · theme_message · memo) · `items`(ContiItem) · `ppt_extras`.

| 콘티 | 큐시트 |
|---|---|
| service_name / date | 큐시트 이름 / 예배 날짜 (YYYY-MM-DD 일 때만) |
| 인도 · 찬양팀 · 주제 · 메모 | 큐시트 메모 |
| 곡 (order 순) | 찬양 순서. 가사: 콘티의 lyrics → 없으면 찬양 라이브러리(제목, 다음 원제의 괄호 앞)에서 |
| song_form | 부를 순서 (`Intro - V1 - C - V2 - C x2 - Bridge` → `1절 후렴 2절 후렴 후렴 브릿지`). 반주만 있는 조각(Intro · Outro · 간주 · 전조 · Interlude)은 뺀다 |
| 송폼이 비었거나 콘티 기본값 `Intro - V1 - C - Outro` | 라이브러리에 저장한 부를 순서 |
| key · time_signature · tempo · song_form · memo | 진행 메모 `Key A · 4/4 · BPM 70 · 송폼 … / 메모` |
| ppt_path (ppt_mode ≠ lyrics, 이 PC 에 파일이 있을 때) | 찬송가·PPT 순서. 파일이 없으면 가사로 넣고 경고 |
| ppt_extras (after = 앞 곡 uid · "" 맨 앞 · `__end__` 맨 끝) | 그 자리의 PPT 순서. 파일이 없으면 제목 순서 + 경고 |
| 같은 split_group 이 이어진 곡(악보를 잘라 나눈 곡) | 한 순서로 |
| 가사를 어디에서도 못 찾은 곡 | 제목 순서 + 경고(가사 찾기 Ctrl+L 안내) |

- 경고는 "큐시트 N개를 가져왔습니다. 확인할 것: …" 한 알림으로 모은다.

## 2. 초성 검색

- `textnorm::fuzzy_contains` · `fuzzy_starts_with` · `has_choseong` · `squash`(NFC + 띄어쓰기 빼기 + 소문자).
- `ㅈㅁㅂㄹ` · `주ㅁㅂㄹ` · `ㅂㄹ ㅂㅈ`(중간부터) 모두 「주만 바라 볼지라」를 찾는다. 맥에서 만든 파일 이름(NFD)도 찾는다.
- 적용: 찬양 라이브러리(`songs::list_songs`, 초성이면 제목만 · 글자면 예전처럼 제목+가사+태그, 제목이 맞는 곡 → 제목이 검색어로 시작하는 곡이 앞), 찬송가·PPT 폴더(`hymns::search_in_folder`, 번호 검색은 그대로), 그림·음악·영상 라이브러리(`library::search_files`).
