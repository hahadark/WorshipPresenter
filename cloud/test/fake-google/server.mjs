// 가짜 구글 (시험 전용): OAuth 2.0 (PKCE) + Drive v3 가운데 워십프리젠터가 쓰는 부분만 흉내 낸다. 진짜 구글은 시험에서 부르지 않는다.
//   node test/fake-google/server.mjs --port 0        → 'fake-google <포트>' 를 찍고 기다린다
//   import { startFakeGoogle } from './server.mjs'   → { port, url, close() }
// 흉내 내는 것: drive.file 범위(이 앱이 만든 파일만 보임) · 사용자마다 따로 · 용량(403 storageQuotaExceeded) · 401 authError ·
//   403 userRateLimitExceeded · 갱신 토큰 거둠(invalid_grant) · multipart · 이어 올리기(resumable) · Range · sha256Checksum · version.
// 시험이 조종하는 길: /_fake/* (아래 control)
import { createServer } from 'node:http';
import { createHash, randomBytes } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export const CLIENT_ID = 'fake-client-id.apps.googleusercontent.com';
export const CLIENT_SECRET = 'fake-client-secret';
const DRIVE_FILE = 'https://www.googleapis.com/auth/drive.file';

const b64u = b => Buffer.from(b).toString('base64url');
const sha256 = b => createHash('sha256').update(b).digest('hex');
const id = (n = 16) => randomBytes(n).toString('base64url').replace(/[-_]/g, 'x').slice(0, n);

export function createState() {
  return { users: new Map(), codes: new Map(), refresh: new Map(), access: new Map(), files: new Map(), uploads: new Map(), consent: new Map(), stats: { calls: {}, crossUser: 0, refreshes: 0, tokenExchanges: 0, rangeReads: 0 } };
}

function userOf(st, email) {
  let u = st.users.get(email);
  if (!u) { u = { email, sub: String(100000 + st.users.size + 1) + sha256(email).slice(0, 8).replace(/\D/g, '1'), quota: 15e9, revoked: false, rate: 0 }; st.users.set(email, u); }
  return u;
}
const usage = (st, email) => [...st.files.values()].filter(f => f.owner === email).reduce((n, f) => n + (f.content?.length || 0), 0);
const driveErr = (code, reason, message) => ({ status: code, body: { error: { code, message, errors: [{ domain: 'global', reason, message }] } } });

function fileJson(f) {
  return {
    kind: 'drive#file', id: f.id, name: f.name, mimeType: f.mimeType, parents: f.parents, trashed: f.trashed, appProperties: f.appProperties,
    version: String(f.version), size: f.content ? String(f.content.length) : undefined, sha256Checksum: f.content ? sha256(f.content) : undefined,
    md5Checksum: f.content ? createHash('md5').update(f.content).digest('hex') : undefined, headRevisionId: f.content ? `r${f.version}` : undefined,
  };
}

/** multipart/related 본문 → { meta, content(Buffer), type } */
function parseMultipart(buf, contentType) {
  const m = /boundary=("?)([^";]+)\1/.exec(contentType || '');
  if (!m) return null;
  const sep = Buffer.from(`--${m[2]}`);
  const parts = [];
  let i = buf.indexOf(sep);
  while (i >= 0) {
    const j = buf.indexOf(sep, i + sep.length);
    if (j < 0) break;
    parts.push(buf.subarray(i + sep.length + 2, j - 2)); // \r\n … \r\n
    i = j;
  }
  const split = p => { const k = p.indexOf('\r\n\r\n'); return { head: p.subarray(0, k).toString(), body: p.subarray(k + 4) }; };
  if (parts.length < 2) return null;
  const a = split(parts[0]), b = split(parts[1]);
  return { meta: JSON.parse(a.body.toString() || '{}'), content: b.body, type: (/content-type:\s*([^\r\n;]+)/i.exec(b.head) || [])[1] || 'application/octet-stream' };
}

function matchesQuery(f, q) {
  if (!q) return true;
  for (const m of q.matchAll(/appProperties has \{ key='([^']*)' and value='((?:[^'\\]|\\.)*)' \}/g)) {
    if ((f.appProperties || {})[m[1]] !== m[2].replace(/\\'/g, "'")) return false;
  }
  if (/trashed\s*=\s*false/.test(q) && f.trashed) return false;
  return true;
}

export function startFakeGoogle({ port = 0, host = '127.0.0.1', accessTtl = 3600 } = {}) {
  const st = createState();
  st.accessTtl = accessTtl;
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = Buffer.concat(chunks);
    const url = new URL(req.url, `http://${req.headers.host}`);
    const key = `${req.method} ${url.pathname.replace(/\/files\/[^/]+/, '/files/:id')}${url.searchParams.get('uploadType') ? `?${url.searchParams.get('uploadType')}` : ''}${url.searchParams.get('alt') ? '?media' : ''}`;
    st.stats.calls[key] = (st.stats.calls[key] || 0) + 1;
    const send = (status, obj, headers = {}) => {
      const text = obj === null ? '' : typeof obj === 'string' || Buffer.isBuffer(obj) ? obj : JSON.stringify(obj);
      res.writeHead(status, { 'content-type': Buffer.isBuffer(obj) ? 'application/octet-stream' : 'application/json; charset=UTF-8', ...headers });
      res.end(text);
    };
    try {
      const out = await handle(st, req, url, body, send, `http://${req.headers.host}`);
      if (out) send(out.status, out.body, out.headers);
    } catch (e) {
      send(500, { error: { code: 500, message: String(e?.stack || e) } });
    }
  });
  return new Promise(resolve => server.listen(port, host, () => {
    const p = server.address().port;
    resolve({ port: p, url: `http://${host}:${p}`, state: st, close: () => new Promise(r => server.close(() => r())) });
  }));
}

async function handle(st, req, url, body, send, origin) {
  const path = url.pathname;
  const q = url.searchParams;
  const form = () => new URLSearchParams(body.toString());
  const jsonBody = () => { try { return JSON.parse(body.toString() || '{}'); } catch { return {}; } };

  // ---------- 시험이 조종하는 길 ----------
  if (path.startsWith('/_fake/')) {
    const a = req.method === 'POST' ? jsonBody() : Object.fromEntries(q);
    const u = a.email ? userOf(st, a.email) : null;
    switch (path) {
      case '/_fake/revoke': u.revoked = true; for (const [k, v] of st.refresh) if (v.email === u.email) st.refresh.delete(k); return { status: 200, body: { ok: true } };
      case '/_fake/quota': u.quota = Number(a.bytes); return { status: 200, body: { ok: true, usage: usage(st, u.email) } };
      case '/_fake/consent': st.consent.set(a.email, { drive: a.drive !== false, deny: !!a.deny }); return { status: 200, body: { ok: true } };
      case '/_fake/rate': u.rate = Number(a.n) || 0; return { status: 200, body: { ok: true } };
      case '/_fake/expire': for (const v of st.access.values()) if (v.email === u.email) v.exp = 0; return { status: 200, body: { ok: true } };
      case '/_fake/purge-file': st.files.delete(a.id); return { status: 200, body: { ok: true } };
      case '/_fake/edit-file': { const f = st.files.get(a.id); f.content = Buffer.from(a.content); f.version++; return { status: 200, body: { ok: true } }; }
      case '/_fake/file': { const f = st.files.get(a.id); return f ? { status: 200, body: f.content || Buffer.alloc(0) } : { status: 404, body: { ok: false } }; }
      case '/_fake/tree': {
        const nameOf = f => { const parts = []; let x = f; while (x) { parts.unshift(x.name); x = x.parents?.[0] ? st.files.get(x.parents[0]) : null; } return parts.join('/'); };
        const list = [...st.files.values()].filter(f => !a.email || f.owner === a.email).map(f => ({ id: f.id, path: nameOf(f), mimeType: f.mimeType, size: f.content?.length ?? null, trashed: f.trashed, appProperties: f.appProperties, owner: f.owner }));
        return { status: 200, body: { ok: true, files: list } };
      }
      case '/_fake/user': return { status: 200, body: { ok: true, sub: u.sub, email: u.email, usage: usage(st, u.email) } };
      case '/_fake/stats': return { status: 200, body: { ok: true, ...st.stats, files: st.files.size } };
      default: return { status: 404, body: { ok: false } };
    }
  }

  // ---------- OAuth ----------
  if (path === '/o/oauth2/v2/auth' && req.method === 'GET') {
    if (q.get('client_id') !== CLIENT_ID) return { status: 400, body: 'invalid_client' };
    if (q.get('code_challenge_method') !== 'S256' || !q.get('code_challenge')) return { status: 400, body: 'pkce required' };
    const ru = q.get('redirect_uri') || '';
    const email = (q.get('login_hint') || 'tester@example.com').toLowerCase();
    const u = userOf(st, email);
    const c = st.consent.get(email) || { drive: true };
    st.consent.delete(email);
    const back = new URL(ru);
    back.searchParams.set('state', q.get('state') || '');
    if (c.deny) { back.searchParams.set('error', 'access_denied'); return { status: 302, body: null, headers: { location: back.toString() } }; }
    let scope = (q.get('scope') || '').split(/\s+/).filter(Boolean);
    if (!c.drive) scope = scope.filter(s => s !== DRIVE_FILE);
    const code = id(24);
    st.codes.set(code, { email: u.email, ru, challenge: q.get('code_challenge'), scope: scope.join(' '), offline: q.get('access_type') === 'offline', exp: Date.now() + 60_000 });
    back.searchParams.set('code', code);
    return { status: 302, body: null, headers: { location: back.toString() } };
  }
  if (path === '/token' && req.method === 'POST') {
    const f = form();
    if (f.get('client_id') !== CLIENT_ID || f.get('client_secret') !== CLIENT_SECRET) return { status: 401, body: { error: 'invalid_client' } };
    const issue = (email, scope) => {
      const at = `ya29.fake-${id(24)}`;
      st.access.set(at, { email, scope, exp: Date.now() + st.accessTtl * 1000 });
      return at;
    };
    if (f.get('grant_type') === 'authorization_code') {
      st.stats.tokenExchanges++;
      const c = st.codes.get(f.get('code'));
      st.codes.delete(f.get('code'));
      if (!c || c.exp < Date.now() || c.ru !== f.get('redirect_uri')) return { status: 400, body: { error: 'invalid_grant' } };
      if (b64u(createHash('sha256').update(f.get('code_verifier') || '').digest()) !== c.challenge) return { status: 400, body: { error: 'invalid_grant', error_description: 'pkce' } };
      const u = userOf(st, c.email);
      u.revoked = false;
      const out = { access_token: issue(u.email, c.scope), expires_in: st.accessTtl, token_type: 'Bearer', scope: c.scope };
      if (c.offline) { const rt = `1//fake-${id(32)}`; st.refresh.set(rt, { email: u.email, scope: c.scope }); out.refresh_token = rt; }
      if (c.scope.includes('openid')) {
        const now = Math.floor(Date.now() / 1000);
        out.id_token = [b64u(JSON.stringify({ alg: 'none', typ: 'JWT' })), b64u(JSON.stringify({ iss: 'https://accounts.google.com', aud: CLIENT_ID, sub: u.sub, email: u.email, email_verified: true, iat: now, exp: now + 3600 })), 'sig'].join('.');
      }
      return { status: 200, body: out };
    }
    if (f.get('grant_type') === 'refresh_token') {
      st.stats.refreshes++;
      const r = st.refresh.get(f.get('refresh_token'));
      if (!r || userOf(st, r.email).revoked) return { status: 400, body: { error: 'invalid_grant', error_description: 'Token has been expired or revoked.' } };
      return { status: 200, body: { access_token: issue(r.email, r.scope), expires_in: st.accessTtl, token_type: 'Bearer', scope: r.scope } };
    }
    return { status: 400, body: { error: 'unsupported_grant_type' } };
  }
  if (path === '/revoke' && req.method === 'POST') {
    const t = q.get('token') || form().get('token');
    st.refresh.delete(t); st.access.delete(t);
    return { status: 200, body: {} };
  }

  // ---------- Drive (Bearer) ----------
  if (!path.startsWith('/drive/v3') && !path.startsWith('/upload/drive/v3')) return { status: 404, body: { error: 'not found' } };
  const auth = /^Bearer (.+)$/.exec(req.headers.authorization || '');
  const tok = auth && st.access.get(auth[1]);
  const user = tok && tok.exp > Date.now() ? userOf(st, tok.email) : null;
  if (!user || user.revoked) return driveErr(401, 'authError', 'Invalid Credentials');
  if (!tok.scope.split(' ').includes(DRIVE_FILE)) return driveErr(403, 'insufficientPermissions', 'Insufficient Permission');
  if (user.rate > 0) { user.rate--; return driveErr(403, 'userRateLimitExceeded', 'User Rate Limit Exceeded'); }
  const mine = fid => {
    const f = st.files.get(fid);
    if (f && f.owner !== user.email) st.stats.crossUser++;
    return f && f.owner === user.email ? f : null;
  };
  const notFound = fid => driveErr(404, 'notFound', `File not found: ${fid}.`);
  const roomFor = n => usage(st, user.email) + n <= user.quota;
  const create = (meta, content, type) => {
    for (const p of meta.parents || []) if (!mine(p)) return notFound(p);
    if (content && !roomFor(content.length)) return driveErr(403, 'storageQuotaExceeded', "The user's Drive storage quota has been exceeded.");
    const f = { id: id(28), name: meta.name || 'Untitled', mimeType: meta.mimeType || type || 'application/octet-stream', parents: meta.parents || [], appProperties: meta.appProperties || {}, trashed: !!meta.trashed, owner: user.email, content: content ?? null, version: 1 };
    st.files.set(f.id, f);
    return { status: 200, body: fileJson(f) };
  };
  const m = /^\/(?:upload\/)?drive\/v3\/files(?:\/([^/?]+))?$/.exec(path);
  if (path === '/drive/v3/about') return { status: 200, body: { storageQuota: { limit: String(user.quota), usage: String(usage(st, user.email)) }, user: { emailAddress: user.email } } };
  if (!m) return { status: 404, body: { error: 'not found' } };
  const fid = m[1] ? decodeURIComponent(m[1]) : null;
  const upload = path.startsWith('/upload/');

  if (!fid && req.method === 'GET' && !upload) {
    const files = [...st.files.values()].filter(f => f.owner === user.email && matchesQuery(f, q.get('q')));
    return { status: 200, body: { files: files.slice(0, Number(q.get('pageSize')) || 100).map(fileJson) } };
  }
  if (!fid && req.method === 'POST' && !upload) return create(JSON.parse(body.toString() || '{}'), null);
  if (!fid && req.method === 'POST' && upload && q.get('uploadType') === 'multipart') {
    const p = parseMultipart(body, req.headers['content-type']);
    if (!p) return driveErr(400, 'badRequest', 'bad multipart');
    return create(p.meta, p.content, p.type);
  }
  if (!fid && req.method === 'POST' && upload && q.get('uploadType') === 'resumable') {
    const meta = JSON.parse(body.toString() || '{}');
    for (const p of meta.parents || []) if (!mine(p)) return notFound(p);
    const len = Number(req.headers['x-upload-content-length']);
    if (Number.isFinite(len) && !roomFor(len)) return driveErr(403, 'storageQuotaExceeded', "The user's Drive storage quota has been exceeded.");
    const u = id(20);
    st.uploads.set(u, { email: user.email, meta, type: req.headers['x-upload-content-type'] });
    return { status: 200, body: '', headers: { location: `${origin}/upload/drive/v3/files?uploadType=resumable&upload_id=${u}&fields=${encodeURIComponent(q.get('fields') || '')}` } };
  }
  if (!fid && req.method === 'PUT' && upload && q.get('upload_id')) {
    const u = st.uploads.get(q.get('upload_id'));
    if (!u || u.email !== user.email) return driveErr(404, 'notFound', 'upload session');
    st.uploads.delete(q.get('upload_id'));
    if (req.headers['content-length'] && Number(req.headers['content-length']) !== body.length) return driveErr(400, 'badContent', 'length');
    return create(u.meta, body, u.type);
  }
  if (!fid) return { status: 404, body: { error: 'not found' } };
  const f = mine(fid);
  if (!f) return notFound(fid);
  if (req.method === 'GET' && q.get('alt') === 'media') {
    const c = f.content || Buffer.alloc(0);
    const r = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range || '');
    if (r) {
      st.stats.rangeReads++;
      let a = r[1] === '' ? c.length - Number(r[2]) : Number(r[1]);
      let b = r[1] === '' || r[2] === '' ? c.length - 1 : Math.min(Number(r[2]), c.length - 1);
      if (a >= c.length || a < 0) return { status: 416, body: '', headers: { 'content-range': `bytes */${c.length}` } };
      return { status: 206, body: c.subarray(a, b + 1), headers: { 'content-range': `bytes ${a}-${b}/${c.length}`, 'content-type': f.mimeType } };
    }
    return { status: 200, body: c, headers: { 'content-type': f.mimeType } };
  }
  if (req.method === 'GET') return { status: 200, body: fileJson(f) };
  if (req.method === 'DELETE') { st.files.delete(fid); return { status: 204, body: null }; }
  if (req.method === 'PATCH') {
    let meta = {}, content = null;
    if (upload && q.get('uploadType') === 'multipart') {
      const p = parseMultipart(body, req.headers['content-type']);
      if (!p) return driveErr(400, 'badRequest', 'bad multipart');
      meta = p.meta; content = p.content;
    } else if (upload && q.get('uploadType') === 'media') content = body;
    else meta = JSON.parse(body.toString() || '{}');
    if (content && !roomFor(content.length - (f.content?.length || 0))) return driveErr(403, 'storageQuotaExceeded', "The user's Drive storage quota has been exceeded.");
    if (meta.name !== undefined) f.name = meta.name;
    if (meta.trashed !== undefined) f.trashed = !!meta.trashed;
    if (meta.appProperties) f.appProperties = { ...f.appProperties, ...meta.appProperties };
    if (content) f.content = content;
    f.version++;
    return { status: 200, body: fileJson(f) };
  }
  return { status: 405, body: { error: 'method' } };
}

// CLI
if (import.meta.url === pathToFileURL(process.argv[1] || '').href) {
  const i = process.argv.indexOf('--port');
  startFakeGoogle({ port: i > 0 ? Number(process.argv[i + 1]) : 0 }).then(s => console.log(`fake-google ${s.port}`));
}
