// 서비스 관리자 길 (Worker 비밀값 ADMIN_TOKEN 이 있을 때만 열림 — 없으면 404). 교회 로그인과 따로다.
//   GET  /api/v1/admin/churches/{id}                 등급 · 옮기기 상태 · 저장소별 수
//   POST /api/v1/admin/churches/{id}/move {to}       무료 ↔ 유료 옮기기 시작 (쓰기 423)
//   POST /api/v1/admin/churches/{id}/move/step       한 단계 (scripts/tier-move.mjs 가 끝날 때까지 부름)
//   POST /api/v1/admin/churches/{id}/move/cancel     그만두기
//   POST /api/v1/admin/churches/{id}/move/cleanup    끝난 뒤 옛 저장소 정리
//   POST /api/v1/admin/demo/reset                    둘러보기를 지금 처음 모습으로
import * as auth from '../auth.mjs';
import { fail, validChurch } from '../protocol.mjs';
import * as move from './move.mjs';
import { adminReset } from './demo.mjs';

export async function handle(request, env, url) {
  const given = (request.headers.get('authorization') || '').replace(/^Bearer\s+/i, '');
  if (!auth.configured(env) || typeof env.ADMIN_TOKEN !== 'string' || env.ADMIN_TOKEN.length < 32) return fail(404, '없는 주소입니다.');
  if (!auth.safeEqual(given, env.ADMIN_TOKEN)) return fail(401, '관리자 열쇠가 맞지 않습니다.');
  const path = url.pathname;
  const method = request.method;
  if (path === '/api/v1/admin/demo/reset' && method === 'POST') return adminReset(env);
  const m = /^\/api\/v1\/admin\/churches\/([a-z0-9-]{2,40})(\/move(?:\/(step|cancel|cleanup))?)?$/.exec(path);
  if (!m || !validChurch(m[1])) return fail(404, '없는 주소입니다.');
  const id = m[1];
  if (!m[2]) return method === 'GET' ? move.info(env, id) : fail(405, 'GET 만 됩니다.');
  if (method !== 'POST') return fail(405, 'POST 만 됩니다.');
  if (!m[3]) { const a = await request.json().catch(() => ({})); return move.start(env, id, a.to); }
  if (m[3] === 'step') return move.step(env, id);
  if (m[3] === 'cancel') return move.cancel(env, id);
  return move.cleanup(env, id);
}
