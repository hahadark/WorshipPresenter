// 2단계 C: 그림 · 영상 · 글꼴을 R2 비공개 저장통에 두고 로그인 뒤에서 내보낸다. 계약: docs/web/assets-api.md
// index.mjs 가 로그인 · 권한(route 의 cap) · 쓰기 보호(X-WorshipCue · Origin)를 본 뒤 handle() 을 부른다.
// 설치판(2B)은 data.bearer 를 거쳐 'Authorization: Bearer 교회.열쇠' 로 온다 (역할 bridge = read · write).
// - 교회는 확인한 로그인(s.church)에서만 온다. 모든 D1 질의는 church_id = ?1 로 시작한다 (무료 범위 합계 둘만 예외 — usage.mjs).
// - R2 열쇠는 churches/{교회}/assets/{sha256}/{이름} — 같은 교회 안에서 같은 내용은 한 번만 저장한다. 다른 교회와는 나누지 않는다.
// - 지우기는 표시만 한다 (지운 파일도 정리 명령 전까지는 id 로 받을 수 있다 — 예배 중 실수로 지워도 화면이 비지 않게).
import { can, fail, json, mediaPath } from '../protocol.mjs';
import { actorOf } from '../data/store.mjs';
import {
  ASSET_LIMITS, CACHE_IMMUTABLE, FILE_CSP, disposition, displayName, fontCss, fontMeta, headerText, normSha,
  parseRange, pathParts, r2Key, resolveType, validAssetId, validSha, validUploadId,
} from './routes.mjs';
import { fontInfo } from './fonts.mjs';
import { addA, countB, guard, quotaResponse, usage } from './usage.mjs';
import { driveBucket } from '../drive/bucket.mjs'; // 7단계 무료 등급: 바이트는 그 교회의 구글 드라이브 (목록 · 이름 · Range 는 이 파일 그대로)
import { DriveError } from '../drive/format.mjs';
import { driveFail } from '../drive/docs.mjs';
import { limitsOf } from '../tiers/limits.mjs';

const ALPHA = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
const rid = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => ALPHA[b % ALPHA.length]).join('');

/** 받기에서 줄을 30초 기억 (같은 그림을 출력 여럿이 받을 때 D1 읽기를 줄임). 고치면 지운다 */
const memo = new Map();
const MEMO_MS = 30_000;
const forget = (church, id) => memo.delete(`${church}/${id}`);

const COLS = `a.id, a.sha256, a.name, a.type, a.kind, a.size, a.r2_key AS r2Key, a.created_at AS createdAt, a.created_by AS createdBy,
  a.updated_at AS updatedAt, a.deleted_at AS deletedAt, a.purged_at AS purgedAt, a.font_family AS fontFamily, a.font_weight AS fontWeight,
  a.font_style AS fontStyle, COALESCE(b.verified, 0) AS verified`;
const FROM = 'FROM assets a LEFT JOIN asset_blobs b ON b.church_id = a.church_id AND b.sha256 = a.sha256';

export function assetJson(r) {
  return {
    id: r.id, sha256: r.sha256, name: r.name, type: r.type, kind: r.kind, size: r.size,
    createdAt: r.createdAt, createdBy: r.createdBy, updatedAt: r.updatedAt ?? null,
    deleted: !!r.deletedAt, deletedAt: r.deletedAt ?? null, purged: !!r.purgedAt, verified: !!r.verified,
    font: r.kind === 'font' ? { family: r.fontFamily ?? null, weight: r.fontWeight ?? null, style: r.fontStyle ?? null } : null,
    url: `/api/v1/assets/${r.id}`,
  };
}

const rowById = (env, church, id) => env.DB.prepare(`SELECT ${COLS} ${FROM} WHERE a.church_id = ?1 AND a.id = ?2`).bind(church, id).first();
/** 같은 내용의 파일 (지우지 않은 것 먼저) */
const rowBySha = (env, church, sha) => env.DB.prepare(
  `SELECT ${COLS} ${FROM} WHERE a.church_id = ?1 AND a.sha256 = ?2 AND a.purged_at IS NULL ORDER BY (a.deleted_at IS NOT NULL), a.created_at LIMIT 1`,
).bind(church, sha).first();

function bucketOf(env, s) {
  if (s?.tier?.store === 'drive') return driveBucket(env, s.church);
  return env.FILES || null;
}
const noBucket = () => fail(503, '파일 저장통(R2)이 연결되어 있지 않습니다.');

export async function handle(request, env, url, s, r) {
  if (!bucketOf(env, s) && !['list', 'usage', 'fonts'].includes(r.op)) return noBucket();
  try {
    return await route(request, env, url, s, r);
  } catch (e) {
    if (e instanceof DriveError) return driveFail(e);
    throw e;
  }
}

async function route(request, env, url, s, r) {
  switch (r.op) {
    case 'list': return list(env, url, s);
    case 'usage': return usage(env, s, can(s.role, 'manage'), json);
    case 'fonts': return fonts(request, env, s);
    case 'upload': return upload(request, env, s);
    case 'start': return start(request, env, s);
    case 'status': return status(env, url, s);
    case 'part': return part(request, env, url, s);
    case 'complete': return complete(env, url, s);
    case 'abort': return abort(env, url, s);
    case 'get': return getAsset(request, env, url, s);
    case 'update': return update(request, env, url, s);
    case 'remove': return remove(env, url, s, true);
    case 'restore': return remove(env, url, s, false);
    case 'purge': return purge(request, env, s);
    default: return fail(404, '없는 주소입니다.');
  }
}

// ---------- 올리기에 공통 ----------

/** 머리글 → { sha, name, t:{type, kind, max}, font } | Response(오류) */
function uploadHeaders(h) {
  const sha = normSha(h.get('x-asset-sha256'));
  if (!validSha(sha)) return fail(400, 'X-Asset-Sha256 머리글(sha256 16진수 64자)이 필요합니다.');
  const name = displayName(headerText(h.get('x-asset-name')));
  const t = resolveType(h.get('content-type'), name);
  if (t.status) return fail(t.status, t.error);
  const font = fontMeta({ family: headerText(h.get('x-asset-font-family')), weight: h.get('x-asset-font-weight'), style: h.get('x-asset-font-style') });
  if (font.error) return fail(400, font.error);
  if (t.kind !== 'font' && Object.keys(font).length) return fail(400, '글꼴 정보는 글꼴 파일에만 적습니다.');
  return { sha, name, t, font };
}

/** 이미 있는 파일 → 그대로 돌려준다 (지운 표시가 있으면 되살린다 — 같은 파일을 다시 올린 것은 다시 쓰겠다는 뜻) */
async function existedReply(env, s, row) {
  if (row.deletedAt) {
    await env.DB.prepare('UPDATE assets SET deleted_at = NULL, updated_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(s.church, row.id, Date.now()).run();
    forget(s.church, row.id);
    row = { ...row, deletedAt: null };
  }
  return json({ ok: true, existed: true, ...uploadReply(row) }, 200);
}
const uploadReply = r => ({ id: r.id, sha256: r.sha256, size: r.size, type: r.type, asset: assetJson(r) });

/** 같은 이름 · 굵기 · 모양 글꼴이 이미 있나 (있으면 알림만 — fonts.css 는 나중 것을 쓴다) */
async function fontClash(env, church, f, exceptId = '') {
  const x = await env.DB.prepare(
    `SELECT id, name FROM assets WHERE church_id = ?1 AND kind = 'font' AND deleted_at IS NULL AND purged_at IS NULL
     AND lower(font_family) = lower(?2) AND font_weight = ?3 AND font_style = ?4 AND id != ?5 LIMIT 1`,
  ).bind(church, f.family, f.weight, f.style, exceptId).first();
  return x ? [`같은 이름 · 굵기 · 모양의 글꼴('${x.name}')이 이미 있습니다. 화면에는 나중에 올린 것을 씁니다.`] : [];
}

/** 글꼴 정보 마무리: 적은 값 → 파일 안에서 읽은 제안 → 파일 이름 */
function finalFont(given, info, name) {
  const sug = info?.suggested || {};
  const f = {
    family: given.family || sug.family || name.replace(/\.[^.]+$/, ''),
    weight: given.weight ?? sug.weight ?? 400,
    style: given.style || sug.style || 'normal',
  };
  const clean = fontMeta(f);
  return clean.error ? { family: name.replace(/\.[^.]+$/, '').replace(/["'\\;{}<>]/g, '').slice(0, 100) || 'font', weight: f.weight, style: f.style } : { ...f, ...clean };
}

/** 줄 둘(물건 · 파일) + A 작업 수 → D1 묶음 하나 */
function insertStatements(env, s, { id, sha, name, t, size, key, verified, font, now, ops }) {
  return [
    env.DB.prepare(
      `INSERT INTO asset_blobs (church_id, sha256, r2_key, size, type, verified, created_at, purged_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
       ON CONFLICT (church_id, sha256) DO UPDATE SET r2_key = excluded.r2_key, size = excluded.size, type = excluded.type,
         verified = MAX(CASE WHEN asset_blobs.purged_at IS NULL THEN asset_blobs.verified ELSE 0 END, excluded.verified), created_at = excluded.created_at, purged_at = NULL
       WHERE asset_blobs.purged_at IS NOT NULL OR excluded.verified > asset_blobs.verified`,
    ).bind(s.church, sha, key, size, t.type, verified ? 1 : 0, now),
    env.DB.prepare(
      `INSERT INTO assets (church_id, id, sha256, name, type, size, r2_key, created_at, deleted_at, kind, created_by, updated_at, font_family, font_weight, font_style)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?8, ?11, ?12, ?13)`,
    ).bind(s.church, id, sha, name, t.type, size, key, now, t.kind, actorOf(s), font?.family ?? null, font?.weight ?? null, font?.style ?? null),
    addA(env, s.church, ops, now),
  ];
}

// ---------- POST /api/v1/assets (한 번에, 90 MiB 까지) ----------
async function upload(request, env, s) {
  const bucket = bucketOf(env, s);
  const u = uploadHeaders(request.headers);
  if (u instanceof Response) return u;
  const { sha, name, t } = u;
  const lenHeader = request.headers.get('content-length');
  if (lenHeader === null) return fail(411, 'Content-Length 가 필요합니다.');
  const size = Number(lenHeader);
  if (!Number.isSafeInteger(size) || size <= 0) return fail(400, '빈 파일입니다.');
  if (size > t.max) return fail(413, `이 종류는 ${Math.floor(t.max / 1048576)} MiB 까지 받습니다.`);
  if (size > ASSET_LIMITS.single) return fail(413, `한 번에는 ${ASSET_LIMITS.single / 1048576} MiB 까지입니다. 나눠 올리기(/api/v1/assets/uploads)를 써 주세요.`, { multipart: true });
  const tl = limitsOf(s.tier?.tier, env); // 7단계: 등급의 파일 하나 한도 (무료 = 나눠 올리기 없음)
  if (tl.fileMax && size > tl.fileMax) return fail(413, `이 등급은 파일 하나 ${Math.floor(tl.fileMax / 1048576)} MiB 까지입니다.`, { code: 'tier_limit', limit: 'fileMax', tier: s.tier?.tier, multipart: false });

  const have = await rowBySha(env, s.church, sha);
  if (have) return existedReply(env, s, have);

  // 무료(드라이브)는 우리 R2 무료 범위를 쓰지 않으므로 안전장치를 보지 않는다 (드라이브가 가득 차면 drive_quota 507)
  const q = bucketOf(env, s).drive ? { ok: true } : await guard(env, s.church, size, 1);
  if (!q.ok) return quotaResponse(json, q);

  let body = request.body, info = null;
  if (t.kind === 'font') {
    // 글꼴은 작으므로 메모리에 읽어 안의 이름 · 굵기를 본다
    const buf = new Uint8Array(await request.arrayBuffer());
    if (buf.length !== size) return fail(400, '받은 크기가 Content-Length 와 다릅니다.');
    info = await fontInfo(buf);
    body = buf;
  }
  const key = r2Key(s.church, sha, name);
  const now = Date.now();
  try {
    // sha256 을 주면 R2 가 받은 바이트로 다시 계산해 다르면 거절한다 (Worker CPU 를 쓰지 않음)
    await bucket.put(key, body, { sha256: sha, httpMetadata: { contentType: t.type }, ...(bucket.drive ? { size } : {}) });
  } catch (e) {
    if (e instanceof DriveError) throw e;
    await addA(env, s.church, 1, now).run().catch(() => {});
    const m = String(e?.message || e);
    if (/digest|sha-?256|checksum/i.test(m)) return fail(400, '받은 내용의 sha256 이 X-Asset-Sha256 과 다릅니다. 다시 올려 주세요.', { reason: 'sha256' });
    console.error('R2 올리기 실패', m);
    return fail(502, '파일 저장통에 쓰지 못했습니다. 잠시 뒤 다시 올려 주세요.');
  }
  const font = t.kind === 'font' ? finalFont(u.font, info, name) : null;
  const id = rid(16);
  await env.DB.batch(insertStatements(env, s, { id, sha, name, t, size, key, verified: true, font, now, ops: 1 }));
  const row = await rowById(env, s.church, id);
  const warnings = font ? [...(u.font.weight === undefined ? info?.warnings || [] : []), ...(await fontClash(env, s.church, font, id))] : [];
  return json({ ok: true, existed: false, ...uploadReply(row), ...(font ? { fontInfo: info && { ...info, warnings: undefined } } : {}), ...(warnings.length ? { warnings } : {}) }, 201);
}

// ---------- 나눠 올리기 (R2 multipart) ----------
async function uploadRow(env, church, id) {
  if (!validUploadId(id)) return null;
  return env.DB.prepare(
    `SELECT id, r2_key AS r2Key, r2_upload AS r2Upload, sha256, name, type, kind, size, part_size AS partSize, parts,
       font_family AS fontFamily, font_weight AS fontWeight, font_style AS fontStyle, created_at AS createdAt FROM asset_uploads WHERE church_id = ?1 AND id = ?2`,
  ).bind(church, id).first();
}
const doneParts = async (env, church, id) => (await env.DB.prepare('SELECT n, etag, size FROM asset_upload_parts WHERE church_id = ?1 AND upload = ?2 ORDER BY n').bind(church, id).all()).results;
const dropUpload = (env, church, id) => [
  env.DB.prepare('DELETE FROM asset_upload_parts WHERE church_id = ?1 AND upload = ?2').bind(church, id),
  env.DB.prepare('DELETE FROM asset_uploads WHERE church_id = ?1 AND id = ?2').bind(church, id),
];
const expected = (u, n) => (n < u.parts ? u.partSize : u.size - u.partSize * (u.parts - 1));

async function start(request, env, s) {
  const u = uploadHeaders(request.headers);
  if (u instanceof Response) return u;
  const { sha, name, t } = u;
  const size = Number(request.headers.get('x-asset-size'));
  if (!Number.isSafeInteger(size) || size <= 0) return fail(400, 'X-Asset-Size 머리글(바이트 수)이 필요합니다.');
  if (size > t.max) return fail(413, `이 종류는 ${Math.floor(t.max / 1048576)} MiB 까지 받습니다.`);
  const partSize = ASSET_LIMITS.part;
  const parts = Math.ceil(size / partSize);
  if (parts > ASSET_LIMITS.parts) return fail(413, '파일이 너무 큽니다.');

  const have = await rowBySha(env, s.church, sha);
  if (have) return existedReply(env, s, have);
  const open = await env.DB.prepare('SELECT COUNT(*) AS n FROM asset_uploads WHERE church_id = ?1').bind(s.church).first();
  if (open.n >= ASSET_LIMITS.uploadsOpen) return fail(429, '나눠 올리는 중인 파일이 너무 많습니다. 끝내거나 그만둔 뒤 다시 해 주세요.');
  const q = await guard(env, s.church, size, parts + 2);
  if (!q.ok) return quotaResponse(json, q);

  const key = r2Key(s.church, sha, name);
  const mp = await bucketOf(env, s).createMultipartUpload(key, { httpMetadata: { contentType: t.type } });
  const id = rid(20), now = Date.now();
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO asset_uploads (church_id, id, r2_key, r2_upload, sha256, name, type, kind, size, part_size, parts, font_family, font_weight, font_style, created_at, created_by)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)`,
    ).bind(s.church, id, key, mp.uploadId, sha, name, t.type, t.kind, size, partSize, parts, u.font.family ?? null, u.font.weight ?? null, u.font.style ?? null, now, actorOf(s)),
    addA(env, s.church, 1, now),
  ]);
  return json({ ok: true, existed: false, uploadId: id, partSize, parts, size }, 201);
}

async function status(env, url, s) {
  const [id] = pathParts(url.pathname).slice(1);
  const u = await uploadRow(env, s.church, id);
  if (!u) return fail(404, '그런 나눠 올리기가 없습니다 (끝났거나 그만뒀습니다).');
  const done = await doneParts(env, s.church, id);
  return json({ ok: true, uploadId: id, sha256: u.sha256, name: u.name, size: u.size, partSize: u.partSize, parts: u.parts, done: done.map(p => p.n), createdAt: u.createdAt });
}

async function part(request, env, url, s) {
  const [id, , nRaw] = pathParts(url.pathname).slice(1);
  const u = await uploadRow(env, s.church, id);
  if (!u) return fail(404, '그런 나눠 올리기가 없습니다 (끝났거나 그만뒀습니다).');
  const n = Number(nRaw);
  if (!Number.isInteger(n) || n < 1 || n > u.parts) return fail(400, `조각 번호는 1~${u.parts} 입니다.`);
  const len = Number(request.headers.get('content-length'));
  if (request.headers.get('content-length') === null) return fail(411, 'Content-Length 가 필요합니다.');
  if (len !== expected(u, n)) return fail(400, `${n}번 조각은 ${expected(u, n)} 바이트여야 합니다.`, { expected: expected(u, n) });
  let p;
  try {
    p = await bucketOf(env, s).resumeMultipartUpload(u.r2Key, u.r2Upload).uploadPart(n, request.body);
  } catch (e) {
    console.error('R2 조각 올리기 실패', e?.message || e);
    return fail(502, '조각을 저장하지 못했습니다. 같은 조각을 다시 올려 주세요.');
  }
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO asset_upload_parts (church_id, upload, n, etag, size) VALUES (?1, ?2, ?3, ?4, ?5)
       ON CONFLICT (church_id, upload, n) DO UPDATE SET etag = excluded.etag, size = excluded.size`,
    ).bind(s.church, id, n, p.etag, len),
    addA(env, s.church, 1),
  ]);
  return json({ ok: true, n, etag: p.etag });
}

async function complete(env, url, s) {
  const [id] = pathParts(url.pathname).slice(1);
  const u = await uploadRow(env, s.church, id);
  if (!u) return fail(404, '그런 나눠 올리기가 없습니다 (끝났거나 그만뒀습니다).');
  const done = await doneParts(env, s.church, id);
  const have = new Set(done.map(p => p.n));
  const missing = [];
  for (let n = 1; n <= u.parts && missing.length < 50; n++) if (!have.has(n)) missing.push(n);
  if (missing.length) return fail(409, '아직 올리지 않은 조각이 있습니다.', { missing });
  const t = { type: u.type, kind: u.kind };
  const mp = bucketOf(env, s).resumeMultipartUpload(u.r2Key, u.r2Upload);
  // 그 사이 같은 내용이 (다른 기기에서) 다 올라왔으면 이 조각들은 버린다
  const blob = await env.DB.prepare('SELECT size, r2_key AS r2Key FROM asset_blobs WHERE church_id = ?1 AND sha256 = ?2 AND purged_at IS NULL').bind(s.church, u.sha256).first();
  const now = Date.now();
  if (blob) {
    await mp.abort().catch(() => {});
    await env.DB.batch(dropUpload(env, s.church, id));
    const row = await rowBySha(env, s.church, u.sha256);
    if (row) return existedReply(env, s, row);
  } else {
    let obj;
    try {
      obj = await mp.complete(done.map(p => ({ partNumber: p.n, etag: p.etag })));
    } catch (e) {
      console.error('R2 나눠 올리기 끝내기 실패', e?.message || e);
      return fail(502, '조각을 합치지 못했습니다. 잠시 뒤 다시 끝내기를 눌러 주세요.');
    }
    if (obj && obj.size !== u.size) {
      await bucketOf(env, s).delete(u.r2Key).catch(() => {});
      await env.DB.batch(dropUpload(env, s.church, id));
      return fail(400, '합친 크기가 알린 크기와 다릅니다. 처음부터 다시 올려 주세요.');
    }
  }
  const font = u.kind === 'font' ? finalFont({ family: u.fontFamily, weight: u.fontWeight ?? undefined, style: u.fontStyle }, null, u.name) : null;
  const aid = rid(16);
  await env.DB.batch([
    ...insertStatements(env, s, { id: aid, sha: u.sha256, name: u.name, t, size: u.size, key: blob ? blob.r2Key : u.r2Key, verified: false, font, now, ops: blob ? 0 : 1 }),
    ...dropUpload(env, s.church, id),
  ]);
  return json({ ok: true, existed: false, ...uploadReply(await rowById(env, s.church, aid)) }, 201);
}

async function abort(env, url, s) {
  const [id] = pathParts(url.pathname).slice(1);
  const u = await uploadRow(env, s.church, id);
  if (!u) return fail(404, '그런 나눠 올리기가 없습니다 (끝났거나 그만뒀습니다).');
  await bucketOf(env, s).resumeMultipartUpload(u.r2Key, u.r2Upload).abort().catch(e => console.error('R2 그만두기 실패', e?.message || e));
  await env.DB.batch(dropUpload(env, s.church, id));
  return json({ ok: true, aborted: true });
}

// ---------- GET /api/v1/assets/{id} (Range) ----------

/** ?fallback=/lib/file?p=… — 클라우드에 없거나 정리된 파일이면 1단계 설치판 중계 길로 돌려보낸다 (그 길만) */
function fallbackOf(url) {
  const f = url.searchParams.get('fallback');
  // 경로 부분의 '\' 는 브라우저가 '/\evil' 을 다른 곳으로 읽을 수 있어 막는다 (물음표 뒤 p=C:\… 는 괜찮음)
  if (!f || !f.startsWith('/') || f.startsWith('//') || f.split('?')[0].includes('\\')) return null;
  let p;
  try { p = new URL(f, url.origin); } catch { return null; }
  if (p.origin !== url.origin || !mediaPath(p.pathname)) return null;
  return new Response(null, { status: 302, headers: { location: p.pathname + p.search, 'cache-control': 'no-store' } });
}

async function getAsset(request, env, url, s) {
  const id = url.pathname.split('/').pop();
  if (!validAssetId(id)) return fail(404, '그런 파일이 없습니다.');
  const mk = `${s.church}/${id}`;
  let hit = memo.get(mk);
  if (!hit || Date.now() - hit.at > MEMO_MS) {
    hit = { row: await rowById(env, s.church, id), at: Date.now() };
    if (memo.size > 1000) memo.clear();
    memo.set(mk, hit);
  }
  const row = hit.row;
  if (!row) return fallbackOf(url) || fail(404, '그런 파일이 없습니다.');
  if (row.purgedAt) return fallbackOf(url) || fail(410, '정리되어 저장통에서 지운 파일입니다.');
  const etag = `"${row.sha256}"`;
  const headers = new Headers({
    'content-type': row.type,
    etag,
    'cache-control': CACHE_IMMUTABLE,
    'accept-ranges': 'bytes',
    'x-content-type-options': 'nosniff',
    'content-security-policy': FILE_CSP,
    'cross-origin-resource-policy': 'same-origin',
    'content-disposition': disposition(row.name, row.kind),
  });
  if (row.deletedAt) headers.set('x-wp-asset-deleted', '1');
  const inm = request.headers.get('if-none-match');
  if (inm && inm.split(',').some(x => x.trim().replace(/^W\//, '') === etag)) return new Response(null, { status: 304, headers });
  const ifRange = request.headers.get('if-range');
  const range = ifRange && ifRange.trim() !== etag ? null : parseRange(request.headers.get('range'), row.size);
  if (range === 'invalid') {
    headers.set('content-range', `bytes */${row.size}`);
    return new Response(null, { status: 416, headers });
  }
  if (request.method === 'HEAD') {
    headers.set('content-length', String(row.size));
    return new Response(null, { status: 200, headers });
  }
  let obj;
  try {
    obj = await bucketOf(env, s).get(row.r2Key, range ? { range } : undefined);
  } catch (e) {
    console.error('R2 받기 실패', e?.message || e);
    return fallbackOf(url) || fail(502, '파일 저장통에서 읽지 못했습니다.');
  }
  await countB(env, s.church);
  if (!obj) return fallbackOf(url) || fail(404, '저장통에 파일이 없습니다.');
  if (range) {
    headers.set('content-range', `bytes ${range.offset}-${range.offset + range.length - 1}/${row.size}`);
    headers.set('content-length', String(range.length));
    return new Response(obj.body, { status: 206, headers });
  }
  headers.set('content-length', String(row.size));
  return new Response(obj.body, { status: 200, headers });
}

// ---------- 목록 · 있나 묻기 ----------
async function list(env, url, s) {
  const q = url.searchParams;
  if (q.has('sha256')) {
    const sha = normSha(q.get('sha256'));
    if (!validSha(sha)) return fail(400, 'sha256 은 16진수 64자입니다.');
    // 지운 표시가 없는 것만 (지운 것만 있으면 null → 다시 올리면 되살아난다)
    const r = await env.DB.prepare(`SELECT ${COLS} ${FROM} WHERE a.church_id = ?1 AND a.sha256 = ?2 AND a.deleted_at IS NULL AND a.purged_at IS NULL ORDER BY a.created_at LIMIT 1`).bind(s.church, sha).first();
    return json({ ok: true, asset: r ? { ...assetJson(r) } : null });
  }
  const kind = q.get('kind') || '';
  if (kind && !['image', 'video', 'audio', 'font', 'pdf'].includes(kind)) return fail(400, '없는 파일 묶음입니다 (image · video · audio · font · pdf).');
  const limit = Math.min(Math.max(Number(q.get('limit')) || ASSET_LIMITS.listDefault, 1), ASSET_LIMITS.listPage);
  const deleted = q.get('deleted') === '1';
  let cAt = Number.MAX_SAFE_INTEGER, cId = '~';
  const cur = q.get('cursor');
  if (cur) {
    const m = /^(\d{1,16})\.([A-Za-z0-9]{16})$/.exec(cur);
    if (!m) return fail(400, 'cursor 가 올바르지 않습니다.');
    cAt = Number(m[1]); cId = m[2];
  }
  const r = await env.DB.prepare(
    `SELECT ${COLS} ${FROM} WHERE a.church_id = ?1 AND a.purged_at IS NULL ${deleted ? '' : 'AND a.deleted_at IS NULL'} AND (?2 = '' OR a.kind = ?2)
     AND (a.created_at < ?3 OR (a.created_at = ?3 AND a.id < ?4)) ORDER BY a.created_at DESC, a.id DESC LIMIT ?5`,
  ).bind(s.church, kind, cAt, cId, limit + 1).all();
  const rows = r.results.slice(0, limit);
  const last = rows[rows.length - 1];
  return json({ ok: true, assets: rows.map(assetJson), next: r.results.length > limit ? `${last.createdAt}.${last.id}` : null });
}

// ---------- 고치기 · 지우기 · 되살리기 ----------
async function update(request, env, url, s) {
  const id = url.pathname.split('/').pop();
  const text = await request.text();
  if (text.length > 4000) return fail(413, '너무 큽니다.');
  let a;
  try { a = JSON.parse(text || '{}'); } catch { return fail(400, '본문이 JSON 이 아닙니다.'); }
  if (!a || typeof a !== 'object' || Array.isArray(a)) return fail(400, '본문은 JSON 객체입니다.');
  const row = await rowById(env, s.church, id);
  if (!row || row.purgedAt) return fail(404, '그런 파일이 없습니다.');
  const name = a.name !== undefined ? displayName(a.name) : row.name;
  const f = fontMeta({ family: a.family, weight: a.weight, style: a.style });
  if (f.error) return fail(400, f.error);
  if (row.kind !== 'font' && Object.keys(f).length) return fail(400, '글꼴 정보는 글꼴 파일에만 적습니다.');
  const font = row.kind === 'font' ? { family: f.family ?? row.fontFamily, weight: f.weight ?? row.fontWeight, style: f.style ?? row.fontStyle } : null;
  await env.DB.prepare('UPDATE assets SET name = ?3, font_family = ?4, font_weight = ?5, font_style = ?6, updated_at = ?7 WHERE church_id = ?1 AND id = ?2')
    .bind(s.church, id, name, font?.family ?? null, font?.weight ?? null, font?.style ?? null, Date.now()).run();
  forget(s.church, id);
  const warnings = font && !row.deletedAt ? await fontClash(env, s.church, font, id) : [];
  return json({ ok: true, asset: assetJson(await rowById(env, s.church, id)), ...(warnings.length ? { warnings } : {}) });
}

async function remove(env, url, s, del) {
  const parts = url.pathname.split('/');
  const id = del ? parts.pop() : parts[parts.length - 2];
  const row = await rowById(env, s.church, id);
  if (!row) return fail(404, '그런 파일이 없습니다.');
  if (row.purgedAt) return fail(410, '정리되어 저장통에서 지운 파일입니다. 다시 올려 주세요.');
  const now = Date.now();
  if (del && !row.deletedAt) await env.DB.prepare('UPDATE assets SET deleted_at = ?3, updated_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(s.church, id, now).run();
  if (!del && row.deletedAt) await env.DB.prepare('UPDATE assets SET deleted_at = NULL, updated_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(s.church, id, now).run();
  forget(s.church, id);
  return json({ ok: true, id, deleted: del, ...(del ? { note: '표시만 했습니다. 정리 명령 전까지는 되살릴 수 있습니다.' } : {}) });
}

// ---------- POST /api/v1/assets/purge (주인, 정리 명령) ----------
async function purge(request, env, s) {
  let a = {};
  try { a = JSON.parse((await request.text()) || '{}'); } catch { return fail(400, '본문이 JSON 이 아닙니다.'); }
  const days = a.olderThanDays === undefined ? ASSET_LIMITS.purgeDays : Number(a.olderThanDays);
  if (!Number.isInteger(days) || days < 0 || days > 3650) return fail(400, 'olderThanDays 는 0~3650 의 정수입니다.');
  if (days === 0 && a.confirm !== 'now') return fail(400, '지운 지 0일 된 것까지 지우려면 confirm:"now" 를 같이 보내 주세요.');
  const dry = a.dryRun !== false; // 기본은 보기만
  const now = Date.now();
  const cutoff = now - days * 86400_000;
  // 살아 있는 파일(지운 표시 없음 · 기다림 안에 지운 것)이 하나도 가리키지 않는 물건만
  const blobs = (await env.DB.prepare(
    `SELECT b.sha256, b.r2_key AS r2Key, b.size, (SELECT group_concat(a.name, ' · ') FROM assets a WHERE a.church_id = ?1 AND a.sha256 = b.sha256) AS names
     FROM asset_blobs b WHERE b.church_id = ?1 AND b.purged_at IS NULL
     AND NOT EXISTS (SELECT 1 FROM assets a WHERE a.church_id = ?1 AND a.sha256 = b.sha256 AND a.purged_at IS NULL AND (a.deleted_at IS NULL OR a.deleted_at > ?2))
     LIMIT 500`,
  ).bind(s.church, cutoff).all()).results;
  const stale = (await env.DB.prepare(
    'SELECT id, r2_key AS r2Key, r2_upload AS r2Upload, size, name FROM asset_uploads WHERE church_id = ?1 AND created_at < ?2 LIMIT 100',
  ).bind(s.church, now - ASSET_LIMITS.uploadTtlMs).all()).results;
  const report = {
    ok: true, dryRun: dry, olderThanDays: days,
    purged: blobs.map(b => ({ sha256: b.sha256, size: b.size, names: b.names || '' })),
    freedBytes: blobs.reduce((n, b) => n + b.size, 0),
    abortedUploads: stale.map(u => ({ uploadId: u.id, name: u.name, size: u.size })),
  };
  if (dry) return json(report);
  const bucket = bucketOf(env, s);
  // R2 지우기 · 나눠 올리기 그만두기는 무료 작업이다
  if (blobs.length) await bucket.delete(blobs.map(b => b.r2Key));
  for (const u of stale) await bucket.resumeMultipartUpload(u.r2Key, u.r2Upload).abort().catch(() => {});
  const stmts = [];
  for (const b of blobs) {
    stmts.push(env.DB.prepare('UPDATE asset_blobs SET purged_at = ?3 WHERE church_id = ?1 AND sha256 = ?2').bind(s.church, b.sha256, now));
    stmts.push(env.DB.prepare('UPDATE assets SET purged_at = ?3 WHERE church_id = ?1 AND sha256 = ?2 AND purged_at IS NULL').bind(s.church, b.sha256, now));
  }
  for (const u of stale) stmts.push(...dropUpload(env, s.church, u.id));
  for (let i = 0; i < stmts.length; i += 40) await env.DB.batch(stmts.slice(i, i + 40));
  memo.clear();
  return json(report);
}

// ---------- GET /api/v1/assets/fonts.css ----------
async function fonts(request, env, s) {
  const r = await env.DB.prepare(
    `SELECT id, font_family AS family, font_weight AS weight, font_style AS style, type, created_at AS createdAt FROM assets
     WHERE church_id = ?1 AND kind = 'font' AND deleted_at IS NULL AND purged_at IS NULL AND font_family IS NOT NULL ORDER BY created_at LIMIT 500`,
  ).bind(s.church).all();
  const css = fontCss(r.results.map(f => ({ ...f, weight: f.weight ?? 400, style: f.style || 'normal' })));
  const h = [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(css)))].slice(0, 12).map(b => b.toString(16).padStart(2, '0')).join('');
  const headers = { 'content-type': 'text/css; charset=utf-8', etag: `"${h}"`, 'cache-control': 'private, max-age=60', 'x-content-type-options': 'nosniff' };
  const inm = request.headers.get('if-none-match');
  if (inm && inm.replace(/^W\//, '').trim() === `"${h}"`) return new Response(null, { status: 304, headers });
  return new Response(request.method === 'HEAD' ? null : css, { status: 200, headers });
}
