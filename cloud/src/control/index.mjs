// 3단계 웹 조작 화면 (web-egui) 의 클라우드 쪽: 페이지 · 성경 한 권 받기.
//   GET /control/                → 조작 화면 (로그인 필요: read 권한 = 주인 · 편집자 · 보기만). wasm · js · 글꼴은 정적 파일 /control/*
//   둘러보기(/demo, tiers/demo.mjs)의 데모 쿠키도 /control/ 을 연다 (demo 역할에 read · source 가 있음, 방문자마다 따로인 중계실)
//   GET /api/v1/control/bible/book?version=&book=  → 그 책 전체 {verses:[[장,절,글]…], headings:[[장,절,글]…]} (read)
// 조작 화면은 설치판 코어(wasm)가 메모리 SQLite 에서 장절 · 낱말을 찾는다 → 번역본을 책 단위로 받아 넣는다(브라우저가 IndexedDB 에 보관,
// 번역본이 바뀌었을 때만 다시 받음). /api/v1/bible/passage 는 한 번에 한 책의 일부라 번역본 전체를 넣기에 맞지 않다.
// 계약: docs/web/architecture.md 5절 · docs/web/egui-web.md
import { fail, json } from '../protocol.mjs';

const VERSION_RE = /^[A-Za-z0-9_.-]{1,40}$/;

/** 길 → { kind:'control', cap, op } · 모르는 길 null */
export function controlRoute(method, path) {
  const get = method === 'GET' || method === 'HEAD';
  if (!get) return null;
  if (path === '/control' || path === '/control/') return { kind: 'page', cap: 'read', page: 'control' };
  if (path === '/api/v1/control/bible/book') return { kind: 'control', cap: 'read', op: 'book' };
  return null;
}


export async function handle(request, env, url, s, r) {
  if (r.op === 'book') return book(env, url, s);
  return fail(404, '없는 주소입니다.');
}

async function book(env, url, s) {
  const v = url.searchParams.get('version') || '';
  const b = Number(url.searchParams.get('book'));
  if (!VERSION_RE.test(v) || !Number.isInteger(b) || b < 1 || b > 66) return fail(400, '번역본 · 책 번호를 확인해 주세요.');
  const ver = await env.DB.prepare('SELECT imported_at AS importedAt FROM bible_versions WHERE church_id = ?1 AND id = ?2').bind(s.church, v).first();
  if (!ver) return fail(404, '선택한 번역본 데이터가 없습니다.');
  const [vs, hs] = await env.DB.batch([
    env.DB.prepare('SELECT chapter AS c, verse AS v, text AS t FROM bible_verses WHERE church_id = ?1 AND version = ?2 AND book = ?3 ORDER BY chapter, verse').bind(s.church, v, b),
    env.DB.prepare('SELECT chapter AS c, verse AS v, text AS t FROM bible_headings WHERE church_id = ?1 AND version = ?2 AND book = ?3 ORDER BY chapter, verse, ord').bind(s.church, v, b),
  ]);
  // 배열로 (JSON 이 작고, Worker CPU 를 덜 씀)
  return json({ ok: true, version: v, book: b, importedAt: ver.importedAt, verses: vs.results.map(x => [x.c, x.v, x.t]), headings: hs.results.map(x => [x.c, x.v, x.t]) },
    200, { 'cache-control': 'private, max-age=300' });
}
