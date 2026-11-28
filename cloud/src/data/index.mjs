// 2단계 A 자료 API 의 입구. index.mjs 가 로그인 · 권한(read/write) · 쓰기 보호를 본 뒤 handle() 을 부른다.
// 설치판(2B)은 쿠키가 없으므로 'Authorization: Bearer 교회.열쇠'(설치판 연결 열쇠)로 bearer() 에 바로 온다.
import * as auth from '../auth.mjs';
import { can, fail } from '../protocol.mjs';
import { getDocument, listDocuments, writeDocument } from './documents.mjs';
import { pull, push } from './sync.mjs';
import { passage, versions } from './bible.mjs';
import { handleImport } from './bibleimport.mjs'; // 7단계 성경 넣기 (등급마다 저장소)
import { tierOf } from '../tiers/index.mjs';
import * as drive from '../drive/docs.mjs'; // 무료 등급: 본문은 구글 드라이브
import * as driveBible from '../drive/bible.mjs';
import { DriveError } from '../drive/format.mjs';

export async function handle(request, env, url, s, r) {
  const t = s.tier || (await tierOf(env, s.church));
  try {
    if (['importVersion', 'importBook', 'deleteVersion'].includes(r.op)) return await handleImport(request, env, url, s, r, t);
    if (t.store === 'drive') return await driveHandle(request, env, url, s, r);
  } catch (e) {
    if (e instanceof DriveError) return drive.driveFail(e);
    throw e;
  }
  switch (r.op) {
    case 'list': return listDocuments(request, env, url, s);
    case 'get': return getDocument(request, env, url, s);
    case 'put': return writeDocument(request, env, url, s, 'put');
    case 'delete': return writeDocument(request, env, url, s, 'delete');
    case 'pull': return pull(request, env, url, s);
    case 'push': return push(request, env, url, s);
    case 'versions': return versions(request, env, url, s);
    case 'passage': return passage(request, env, url, s);
    default: return fail(404, '없는 주소입니다.');
  }
}

/** 무료 등급 (구글 드라이브) — 같은 길, 같은 답 모양 */
async function driveHandle(request, env, url, s, r) {
  switch (r.op) {
    case 'list': return drive.listDocuments(request, env, url, s);
    case 'get': return drive.getDocument(request, env, url, s);
    case 'put': return drive.writeDocument(request, env, url, s, 'put');
    case 'delete': return drive.writeDocument(request, env, url, s, 'delete');
    case 'pull': return drive.pull(request, env, url, s);
    case 'push': return drive.push(request, env, url, s);
    case 'versions': return driveBible.versions(request, env, url, s);
    case 'passage': return driveBible.passage(request, env, url, s);
    default: return fail(404, '없는 주소입니다.');
  }
}

// 설치판 열쇠 확인은 요청마다 D1 을 두 번 쓰게 되므로(해시 찾기 · 마지막 사용 시각) 30초 기억한다.
// 기억한 동안에도 열쇠가 살아 있는지는 auth.credentialAlive 가 (30초 기억으로) 다시 본다 → 지운 열쇠는 늦어도 30초 안에 막힌다.
const seen = new Map(); // sha256(머리글) → { church, cred, at }
const SEEN_MS = 30_000;
async function digest(text) {
  return [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)))].map(b => b.toString(16).padStart(2, '0')).join('');
}

export async function bearer(request, env, url, r, next = handle) { // next: 2C 파일 길도 같은 열쇠 확인을 쓴다
  const header = (request.headers.get('authorization') || '').replace(/^Bearer\s+/i, '');
  const key = auth.splitKey(header);
  if (!key) return fail(401, '설치판 연결 열쇠가 필요합니다.');
  const h = await digest(header);
  let hit = seen.get(h);
  const now = Date.now();
  if (!hit || now - hit.at > SEEN_MS) {
    const v = await auth.verify(env, key.church, key.secret, ['bridge'], request.headers.get('cf-connecting-ip') || 'local');
    if (!v.ok) { seen.delete(h); return fail(v.status, v.error); }
    hit = { church: key.church, cred: v.cred, at: now };
    if (seen.size > 2000) seen.clear();
    seen.set(h, hit);
  } else if (!(await auth.credentialAlive(env, hit.church, hit.cred))) {
    seen.delete(h);
    return fail(401, '교회 이름이나 열쇠(PIN)가 맞지 않습니다.');
  }
  const s = { church: hit.church, role: 'bridge', cred: hit.cred, exp: 0 };
  if (r.cap && !can('bridge', r.cap)) return fail(403, '이 기기에는 그 권한이 없습니다.');
  return next(request, env, url, s, r);
}
