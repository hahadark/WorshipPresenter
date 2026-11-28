// 무료 등급 파일: 2C(assets/index.mjs) 가 R2 저장통(env.FILES)을 부르는 자리에 이것을 끼운다 — 목록 · 이름 · sha256 · 글꼴 정보 ·
// Range · 지우기 표시 · fonts.css 는 2C 코드 그대로, 바이트만 그 교회의 구글 드라이브 '파일/<묶음>/' 에 원래 형식으로 둔다.
// R2 열쇠(churches/{교회}/assets/{sha256}/{이름}) → 드라이브 파일 id 는 D1 drive_files 에 (작은 것만).
// - 올리기: 이어 올리기(resumable) 두 번 부르기. 본문은 Worker 가 읽지 않고 흘려 보낸다 (CPU 10 ms · 하위 요청 2개).
// - sha256: 드라이브가 계산한 sha256Checksum 과 비교, 다르면 그 파일을 지우고 오류 (2C 의 'sha256' 오류로 이어짐).
// - 받기: 드라이브의 Range 를 그대로 흘려 보낸다 (응답 크기 한도 없음, CPU 거의 안 씀).
import { accessToken, drive, endpoints } from './google.mjs';
import { DriveError, driveErrorOf } from './format.mjs';
import { folderFor } from './folders.mjs';

const MiB = 1024 * 1024;
const kindOfType = t => (t.startsWith('image/') ? 'image' : t.startsWith('video/') ? 'video' : t.startsWith('audio/') ? 'audio' : t.startsWith('font/') ? 'font' : t === 'application/pdf' ? 'pdf' : 'image');

const memo = new Map(); // `${church}/${key}` → { fileId, at }
const MEMO_MS = 5 * 60_000;

async function fileIdOf(env, church, key) {
  const k = `${church}/${key}`;
  const hit = memo.get(k);
  if (hit && Date.now() - hit.at < MEMO_MS) return hit.fileId;
  const r = await env.DB.prepare('SELECT file_id AS f FROM drive_files WHERE church_id = ?1 AND r2_key = ?2').bind(church, key).first();
  if (memo.size > 2000) memo.clear();
  memo.set(k, { fileId: r?.f || null, at: Date.now() });
  return r?.f || null;
}

/** R2 의 put · get · delete 꼴. 나눠 올리기(createMultipartUpload)는 없다 — 무료 등급은 tiers/limits.mjs gate 가 먼저 막는다 */
export function driveBucket(env, church) {
  return {
    drive: true,
    async put(key, body, opts = {}) {
      const type = opts.httpMetadata?.contentType || 'application/octet-stream';
      const bytes = body instanceof Uint8Array ? body : null;
      const size = bytes ? bytes.length : Number(opts.size);
      if (!bytes && !Number.isSafeInteger(size)) throw new Error('크기를 모릅니다');
      const name = key.split('/').pop();
      const sha = opts.sha256 || '';
      const parent = await folderFor(env, church, `files:${kindOfType(type)}`);
      // 1) 이어 올리기 시작 (메타)
      const init = await drive(env, church, 'upload:/files?uploadType=resumable&fields=id,size,sha256Checksum', {
        method: 'POST',
        headers: { 'content-type': 'application/json; charset=UTF-8', 'x-upload-content-type': type, 'x-upload-content-length': String(size) },
        body: JSON.stringify({ name, parents: [parent], mimeType: type, appProperties: { wpSha256: sha } }),
      }, { raw: true });
      const loc = init.headers.get('location');
      if (!loc || !(loc.startsWith(endpoints(env).upload) || loc.startsWith('https://www.googleapis.com/upload/'))) throw new DriveError('drive_unavailable', '구글 드라이브가 올릴 곳을 주지 않았습니다.');
      // 2) 바이트 (흘려 보냄 — 길이를 알려야 구글이 받는다)
      let stream = bytes;
      if (!bytes) {
        if (typeof FixedLengthStream === 'function') { const fl = new FixedLengthStream(size); body.pipeTo(fl.writable).catch(() => {}); stream = fl.readable; } else stream = body;
      }
      const token = await accessToken(env, church);
      let r;
      try {
        r = await fetch(loc, { method: 'PUT', headers: { authorization: `Bearer ${token}`, 'content-type': type, 'content-length': String(size) }, body: stream });
      } catch { throw new DriveError('drive_unavailable', '구글 드라이브에 올리지 못했습니다.'); }
      if (!r.ok) throw driveErrorOf(r.status, await r.json().catch(() => ({})));
      const f = await r.json().catch(() => ({}));
      if (sha && f.sha256Checksum && f.sha256Checksum.toLowerCase() !== sha) {
        await drive(env, church, `/files/${encodeURIComponent(f.id)}`, { method: 'DELETE' }).catch(() => null);
        throw new Error('sha256 digest mismatch');
      }
      await env.DB.prepare('INSERT INTO drive_files (church_id, r2_key, file_id, size, sha256, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (church_id, r2_key) DO UPDATE SET file_id = excluded.file_id, size = excluded.size')
        .bind(church, key, f.id, size, sha, Date.now()).run();
      memo.set(`${church}/${key}`, { fileId: f.id, at: Date.now() });
      return { key, size, verified: !!f.sha256Checksum };
    },
    async get(key, opts = {}) {
      const fileId = await fileIdOf(env, church, key);
      if (!fileId) return null;
      const headers = {};
      if (opts.range) headers.range = `bytes=${opts.range.offset}-${opts.range.offset + opts.range.length - 1}`;
      try {
        const r = await drive(env, church, `/files/${encodeURIComponent(fileId)}?alt=media`, { headers }, { raw: true });
        return { body: r.body, size: Number(r.headers.get('content-length')) || undefined };
      } catch (e) {
        if (e instanceof DriveError && e.code === 'drive_missing') return null;
        throw e;
      }
    },
    async delete(keys) {
      for (const key of Array.isArray(keys) ? keys : [keys]) {
        const fileId = await fileIdOf(env, church, key);
        if (!fileId) continue;
        // 휴지통으로 (사람이 드라이브에서 되살릴 수 있게)
        await drive(env, church, `/files/${encodeURIComponent(fileId)}?fields=id`, { method: 'PATCH', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ trashed: true }) }).catch(() => null);
        await env.DB.prepare('DELETE FROM drive_files WHERE church_id = ?1 AND r2_key = ?2').bind(church, key).run();
        memo.delete(`${church}/${key}`);
      }
    },
    createMultipartUpload() { throw new DriveError('drive_unavailable', '무료 등급은 나눠 올리기가 없습니다.'); },
    resumeMultipartUpload() { throw new DriveError('drive_unavailable', '무료 등급은 나눠 올리기가 없습니다.'); },
  };
}

export const FREE_SINGLE_MAX = 90 * MiB;
