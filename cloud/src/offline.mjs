// 6단계 오프라인: 조작 화면(/control/) 머리에 끼우는 것 — 서비스 워커 등록 · IndexedDB 보관함 (client/offline/offline.js, docs/web/offline.md)
// 로그인한 조작 화면에만 (둘러보기는 견본 교회를 여럿이 같이 보므로 보관하지 않는다). 출력 페이지에는 끼우지 않는다 —
// 같은 브라우저의 출력 창은 조작 화면이 등록한 서비스 워커가 맡고, 다른 PC(OBS 등)는 1단계처럼 마지막 화면을 유지한다.
export const offlineTag = cfg => (cfg && cfg.authed && cfg.role !== 'demo' ? '<script src="/cloud/offline/offline.js"></script>' : '');

/** 로그아웃 단추가 있는 페이지(주인 · 권한 없음): 로그아웃 전에 못 올린 것을 묻고 이 브라우저 보관을 지우는 helper 만 (설정이 없으니 서비스 워커는 등록하지 않음) */
export const logoutTag = '<script src="/cloud/offline/offline.js"></script>';
