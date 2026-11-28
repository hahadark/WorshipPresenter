// 2단계 C 단위 시험 (Cloudflare 없이 node 로): 길 · 권한 · 파일 종류 · 이름 · R2 열쇠 · Range · 무료 범위 안전장치 · 글꼴 CSS · 글꼴 파일 읽기
import test from 'node:test';
import assert from 'node:assert/strict';
import { can, route } from '../src/protocol.mjs';
import {
  ASSET_LIMITS, assetRoute, displayName, fontCss, fontMeta, freeLimits, headerText, monthOf, parseRange, quotaDecision, r2Key,
  resolveType, safeKeyName, validAssetId, validSha, disposition,
} from '../src/assets/routes.mjs';
import { fontInfo, weightFromName } from '../src/assets/fonts.mjs';

const q = s => new URLSearchParams(s);

test('2C 길: route() 에 이어 붙고, 받기는 그 교회 로그인이면 누구나 · 올리기는 write · 정리는 manage', () => {
  const id = 'A'.repeat(16), up = 'b'.repeat(20);
  assert.deepEqual(route('GET', '/api/v1/assets', q('')), { kind: 'assets', cap: 'read', op: 'list' });
  assert.deepEqual(route('POST', '/api/v1/assets', q('')), { kind: 'assets', cap: 'write', op: 'upload' });
  assert.deepEqual(route('GET', `/api/v1/assets/${id}`, q('')), { kind: 'assets', cap: null, op: 'get' });
  assert.deepEqual(route('HEAD', `/api/v1/assets/${id}`, q('')), { kind: 'assets', cap: null, op: 'get' });
  assert.equal(route('PATCH', `/api/v1/assets/${id}`, q('')).cap, 'write');
  assert.equal(route('DELETE', `/api/v1/assets/${id}`, q('')).op, 'remove');
  assert.equal(route('POST', `/api/v1/assets/${id}/restore`, q('')).op, 'restore');
  assert.equal(route('PUT', `/api/v1/assets/${id}`, q('')), null);
  assert.equal(route('GET', '/api/v1/assets/short', q('')), null);
  assert.equal(route('GET', '/api/v1/assets/fonts.css', q('')).cap, null);
  assert.equal(route('GET', '/api/v1/assets/usage', q('')).cap, 'read');
  assert.equal(route('POST', '/api/v1/assets/purge', q('')).cap, 'manage');
  assert.equal(route('GET', '/api/v1/assets/purge', q('')), null);
  assert.equal(route('POST', '/api/v1/assets/uploads', q('')).op, 'start');
  assert.equal(route('GET', `/api/v1/assets/uploads/${up}`, q('')).op, 'status');
  assert.equal(route('DELETE', `/api/v1/assets/uploads/${up}`, q('')).op, 'abort');
  assert.equal(route('PUT', `/api/v1/assets/uploads/${up}/parts/3`, q('')).op, 'part');
  assert.equal(route('POST', `/api/v1/assets/uploads/${up}/parts/3`, q('')), null);
  assert.equal(route('POST', `/api/v1/assets/uploads/${up}/complete`, q('')).op, 'complete');
  assert.equal(assetRoute('GET', '/api/v1/documents'), null);
  // 권한 표: 올리기는 owner · 사람 editor · 설치판 bridge, 보기만 사람은 목록만, 출력 · PIN 기기는 목록도 못 봄
  for (const r of ['owner', 'member-editor', 'bridge']) assert.ok(can(r, 'write'), r);
  for (const r of ['member-viewer', 'output', 'remote', 'editor']) assert.ok(!can(r, 'write'), r);
  for (const r of ['output', 'remote', 'editor']) assert.ok(!can(r, 'read'), r);
  assert.ok(!can('bridge', 'manage') && !can('member-editor', 'manage') && can('owner', 'manage'));
});

test('파일 종류: 받는 것만 · 확장자로 정하기 · 서로 다르면 거절', () => {
  assert.deepEqual(resolveType('image/png', 'a.png'), { type: 'image/png', kind: 'image', max: 50 * 1048576 });
  assert.equal(resolveType('application/octet-stream', '배경.JPG').type, 'image/jpeg');
  assert.equal(resolveType('', 'x.woff2').type, 'font/woff2');
  assert.equal(resolveType('application/x-font-ttf', 'x.ttf').type, 'font/ttf');
  assert.equal(resolveType('video/mp4; codecs=avc1', 'x.mp4').kind, 'video');
  assert.equal(resolveType('audio/x-wav', 'x.wav').type, 'audio/wav');
  assert.equal(resolveType('text/html', 'x.html').status, 415);
  assert.equal(resolveType('application/javascript', 'x.js').status, 415);
  assert.equal(resolveType('', 'noext').status, 415);
  assert.equal(resolveType('image/png', 'x.mp4').status, 415);
  assert.equal(resolveType('video/mp4', 'x.mp4').max, 2048 * 1048576);
});

test('PowerPoint: pptx · ppsx · ppt · pps 를 받고, 내려받기만 (attachment)', () => {
  const PPTX = 'application/vnd.openxmlformats-officedocument.presentationml.presentation';
  assert.deepEqual(resolveType(PPTX, '광고.pptx'), { type: PPTX, kind: 'slides', max: 500 * 1048576 });
  assert.equal(resolveType('application/octet-stream', '찬양.PPSX').type, 'application/vnd.openxmlformats-officedocument.presentationml.slideshow');
  assert.equal(resolveType('', '옛 슬라이드.ppt').type, 'application/vnd.ms-powerpoint');
  assert.equal(resolveType('', '쇼.pps').type, 'application/vnd.ms-powerpoint');
  assert.equal(resolveType('application/mspowerpoint', 'x.pps').kind, 'slides');
  assert.equal(resolveType(PPTX, 'x.png').status, 415, '종류와 확장자가 다르면 거절');
  assert.equal(resolveType('application/vnd.ms-powerpoint.presentation.macroEnabled.12', 'x.pptm').status, 415, '매크로가 든 것은 받지 않음');
  assert.match(disposition('광고.pptx', 'slides'), /^attachment; /);
  assert.match(disposition('a.pdf', 'pdf'), /^inline; /);
});

test('이름: 경로 · 사용자 이름을 떼고, R2 열쇠는 교회/sha256/안전한 이름', () => {
  assert.equal(displayName('C:\\Users\\김택근\\Pictures\\부활절 배경.png'), '부활절 배경.png');
  assert.equal(displayName('/Users/kim/Movies/a.mp4'), 'a.mp4');
  assert.equal(displayName('a\u0000b\n.png'), 'ab.png');
  assert.equal(displayName(''), 'file');
  assert.equal(displayName('가'.repeat(300) + '.png').length, 200);
  assert.ok(displayName('가'.repeat(300) + '.png').endsWith('.png'));
  assert.equal(safeKeyName('부활절 배경 (1).png'), '부활절_배경_1_.png');
  assert.equal(safeKeyName('../../etc/passwd'), 'passwd');
  assert.equal(safeKeyName('...'), 'file');
  const sha = 'a'.repeat(64);
  assert.equal(r2Key('sample', sha, 'C:\\x\\부활절.png'), `churches/sample/assets/${sha}/부활절.png`);
  assert.equal(headerText(encodeURIComponent('부활절 배경.png')), '부활절 배경.png');
  assert.equal(headerText('%E0%A4%A'), '%E0%A4%A');
  assert.ok(validSha(sha) && !validSha('A'.repeat(64)) && !validSha('a'.repeat(63)));
  assert.ok(validAssetId('Abc123Abc123Abc1') && !validAssetId('abc'));
  assert.match(disposition('부활절.png', 'image'), /^inline; filename="___\.png"; filename\*=UTF-8''%EB%B6%80/);
});

test('Range: 한 범위 · 끝에서 n · 열린 끝 · 범위 밖은 416 · 여러 범위는 전체', () => {
  assert.equal(parseRange(null, 100), null);
  assert.deepEqual(parseRange('bytes=0-9', 100), { offset: 0, length: 10 });
  assert.deepEqual(parseRange('bytes=90-', 100), { offset: 90, length: 10 });
  assert.deepEqual(parseRange('bytes=-5', 100), { offset: 95, length: 5 });
  assert.deepEqual(parseRange('bytes=-500', 100), { offset: 0, length: 100 });
  assert.deepEqual(parseRange('bytes=50-1000', 100), { offset: 50, length: 50 });
  assert.equal(parseRange('bytes=100-', 100), 'invalid');
  assert.equal(parseRange('bytes=9-3', 100), 'invalid');
  assert.equal(parseRange('bytes=-0', 100), 'invalid');
  assert.equal(parseRange('bytes=-', 100), 'invalid');
  assert.equal(parseRange('items=0-1', 100), 'invalid');
  assert.equal(parseRange('bytes=0-1,5-6', 100), null);
});

test('무료 범위 안전장치: 저장 · A 작업 80% · 교회 한도', () => {
  const L = freeLimits({});
  assert.deepEqual(L, { bytes: 10e9, classA: 1e6, classB: 1e7, ratio: 0.8 });
  const base = { storedBytes: 0, reservedBytes: 0, addBytes: 0, classA: 0, addA: 1, churchBytes: 0, churchLimit: null };
  assert.deepEqual(quotaDecision({ ...base, addBytes: 1e9 }, L), { ok: true });
  assert.deepEqual(quotaDecision({ ...base, storedBytes: 7.5e9, addBytes: 0.5e9 }, L), { ok: true });
  assert.deepEqual(quotaDecision({ ...base, storedBytes: 7.5e9, addBytes: 0.5e9 + 1 }, L), { ok: false, reason: 'storage' });
  assert.deepEqual(quotaDecision({ ...base, storedBytes: 7e9, reservedBytes: 1e9, addBytes: 1 }, L), { ok: false, reason: 'storage' });
  assert.deepEqual(quotaDecision({ ...base, classA: 799_999 }, L), { ok: true });
  assert.deepEqual(quotaDecision({ ...base, classA: 799_999, addA: 3 }, L), { ok: false, reason: 'classA' });
  assert.deepEqual(quotaDecision({ ...base, churchBytes: 900, churchLimit: 1000, addBytes: 101 }, L), { ok: false, reason: 'church' });
  assert.deepEqual(freeLimits({ ASSETS_FREE_BYTES: '1000', ASSETS_GUARD_RATIO: '5' }).ratio, 1);
  assert.equal(freeLimits({ ASSETS_FREE_BYTES: 'x' }).bytes, 10e9);
  assert.equal(monthOf(Date.UTC(2026, 10, 30, 23, 59)), '2026-11');
  assert.ok(ASSET_LIMITS.single < 100e6, '한 번에 올리기는 Workers 무료 요청 본문(100 MB) 아래');
  assert.ok(ASSET_LIMITS.part >= ASSET_LIMITS.partMin && ASSET_LIMITS.part < 100e6);
});

test('글꼴 정보: 이름 · 굵기 · 모양 확인', () => {
  assert.deepEqual(fontMeta({ family: '구세군체', weight: '700', style: 'Normal' }), { family: '구세군체', weight: 700, style: 'normal' });
  assert.deepEqual(fontMeta({}), {});
  assert.ok(fontMeta({ weight: 1001 }).error);
  assert.ok(fontMeta({ weight: 1.5 }).error);
  assert.ok(fontMeta({ style: 'bold' }).error);
  assert.equal(fontMeta({ family: 'a"}b' }).family, 'ab');
  assert.ok(fontMeta({ family: '"' }).error);
});

test('글꼴 CSS: 이름 · 굵기를 적은 대로, 굵기가 하나뿐인 이름은 모든 굵기에, 겹치면 나중 것', () => {
  const css = fontCss([
    { id: 'R'.repeat(16), family: '구세군체', weight: 400, style: 'normal', type: 'font/ttf', createdAt: 1 },
    { id: 'B'.repeat(16), family: '구세군체', weight: 700, style: 'normal', type: 'font/ttf', createdAt: 2 },
    { id: 'S'.repeat(16), family: '구세군체 Bold', weight: 400, style: 'normal', type: 'font/woff2', createdAt: 3 },
    { id: 'O'.repeat(16), family: 'Old', weight: 400, style: 'normal', type: 'font/otf', createdAt: 4 },
    { id: 'N'.repeat(16), family: 'old', weight: 400, style: 'normal', type: 'font/otf', createdAt: 5 },
  ]);
  assert.match(css, new RegExp(`font-family:"구세군체";font-weight:400;font-style:normal;font-display:block;src:url\\("/api/v1/assets/${'R'.repeat(16)}"\\) format\\("truetype"\\)`));
  assert.match(css, /font-family:"구세군체";font-weight:700;/);
  assert.match(css, /font-family:"구세군체 Bold";font-weight:1 1000;.*format\("woff2"\)/);
  assert.ok(!css.includes('O'.repeat(16)) && css.includes('N'.repeat(16)), '같은 이름(대소문자 무시) · 굵기 · 모양은 나중 것만');
  assert.equal(fontCss([]).trim(), '/* 워십프리젠터 교회 글꼴 (자동으로 만든 것) */');
});

// ---------- 글꼴 파일 (시험용으로 만든 작은 sfnt: name · OS/2 표만) ----------
function sfnt({ names, weight = 400, italic = false, otto = false }) {
  const enc = s => { const b = []; for (const ch of s) { const c = ch.charCodeAt(0); b.push(c >> 8, c & 255); } return b; };
  // name 표
  const recs = [], strs = [];
  let off = 0;
  for (const [id, lang, text] of names) { const b = enc(text); recs.push([3, 1, lang, id, b.length, off]); strs.push(...b); off += b.length; }
  const name = [0, 0, recs.length >> 8, recs.length & 255, (6 + recs.length * 12) >> 8, (6 + recs.length * 12) & 255];
  for (const r of recs) for (const v of r) name.push(v >> 8, v & 255);
  name.push(...strs);
  const os2 = new Array(96).fill(0);
  os2[4] = weight >> 8; os2[5] = weight & 255;
  os2[62] = 0; os2[63] = italic ? 1 : 0;
  const tables = [['OS/2', os2], ['name', name]];
  const head = 12 + tables.length * 16;
  const out = [];
  const u32 = v => [(v >>> 24) & 255, (v >>> 16) & 255, (v >>> 8) & 255, v & 255];
  out.push(...(otto ? [0x4f, 0x54, 0x54, 0x4f] : u32(0x00010000)), 0, tables.length, 0, 0, 0, 0, 0, 0);
  let pos = head;
  const bodies = [];
  for (const [tag, data] of tables) {
    out.push(...[...tag].map(c => c.charCodeAt(0)), 0, 0, 0, 0, ...u32(pos), ...u32(data.length));
    bodies.push(...data); pos += data.length;
  }
  return new Uint8Array([...out, ...bodies]);
}
/** 같은 표를 WOFF 1.0 으로 (name 은 deflate 로 눌러서) */
async function woff(bytes) {
  const dv = new DataView(bytes.buffer);
  const n = dv.getUint16(4);
  const tabs = [];
  for (let i = 0; i < n; i++) {
    const o = 12 + i * 16;
    const tag = String.fromCharCode(...bytes.subarray(o, o + 4));
    const data = bytes.subarray(dv.getUint32(o + 8), dv.getUint32(o + 8) + dv.getUint32(o + 12));
    const comp = tag === 'name' ? new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new CompressionStream('deflate'))).arrayBuffer()) : data;
    tabs.push({ tag, data, comp });
  }
  const headLen = 44 + tabs.length * 20;
  const out = new Uint8Array(headLen + tabs.reduce((s, t) => s + t.comp.length, 0));
  const w = new DataView(out.buffer);
  out.set([0x77, 0x4f, 0x46, 0x46]); w.setUint32(4, 0x00010000); w.setUint32(8, out.length); w.setUint16(12, tabs.length);
  let pos = headLen;
  tabs.forEach((t, i) => {
    const o = 44 + i * 20;
    out.set([...t.tag].map(c => c.charCodeAt(0)), o);
    w.setUint32(o + 4, pos); w.setUint32(o + 8, t.comp.length); w.setUint32(o + 12, t.data.length);
    out.set(t.comp, pos); pos += t.comp.length;
  });
  return out;
}

test('글꼴 파일 읽기: 구세군체 Bold 처럼 이름은 Bold · 파일 굵기 400 이면 700 으로 제안하고 알린다', async () => {
  const f = sfnt({ names: [[1, 0x0409, 'Salvation Army'], [1, 0x0412, '구세군체'], [2, 0x0409, 'Bold'], [4, 0x0412, '구세군체 Bold']], weight: 400 });
  const i = await fontInfo(f);
  assert.equal(i.format, 'ttf');
  assert.equal(i.family, '구세군체');
  assert.equal(i.familyEn, 'Salvation Army');
  assert.equal(i.subfamily, 'Bold');
  assert.equal(i.weightClass, 400);
  assert.deepEqual(i.suggested, { family: '구세군체', weight: 700, style: 'normal' });
  assert.equal(i.warnings.length, 1);
  assert.match(i.warnings[0], /400.*700/);
});

test('글꼴 파일 읽기: 보통 글꼴 · 기울임 · OTF · WOFF · WOFF2 · 못 읽는 파일', async () => {
  const reg = await fontInfo(sfnt({ names: [[1, 0x0409, 'Noto Sans KR'], [2, 0x0409, 'Regular']], weight: 400 }));
  assert.deepEqual(reg.suggested, { family: 'Noto Sans KR', weight: 400, style: 'normal' });
  assert.deepEqual(reg.warnings, []);
  const it = await fontInfo(sfnt({ names: [[1, 0x0409, 'X'], [2, 0x0409, 'Medium Italic']], weight: 500, italic: true, otto: true }));
  assert.equal(it.format, 'otf');
  assert.deepEqual(it.suggested, { family: 'X', weight: 500, style: 'italic' });
  // 굵기 차이가 작으면(Semibold 600 ↔ 파일 650) 그대로 둔다
  assert.equal((await fontInfo(sfnt({ names: [[1, 0x0409, 'Y'], [2, 0x0409, 'SemiBold']], weight: 650 }))).suggested.weight, 650);
  // 이름 1번이 '구세군체 Bold' (가족을 따로 만든 글꼴) 이면 그 이름 그대로 — 템플릿에 그렇게 적힌다
  const sep = await fontInfo(sfnt({ names: [[1, 0x0412, '구세군체 Bold'], [2, 0x0409, 'Regular'], [16, 0x0412, '구세군체']], weight: 400 }));
  assert.equal(sep.suggested.family, '구세군체 Bold');
  assert.equal(sep.typoFamily, '구세군체');
  const w = await fontInfo(await woff(sfnt({ names: [[1, 0x0412, '구세군체'], [2, 0x0409, 'Bold']], weight: 400 })));
  assert.equal(w.format, 'woff');
  assert.deepEqual(w.suggested, { family: '구세군체', weight: 700, style: 'normal' });
  const w2 = await fontInfo(new Uint8Array([0x77, 0x4f, 0x46, 0x32, 0, 0, 0, 0]));
  assert.equal(w2.format, 'woff2');
  assert.equal(w2.warnings.length, 1);
  const bad = await fontInfo(new Uint8Array([1, 2, 3]));
  assert.equal(bad.format, 'unknown');
  assert.deepEqual(bad.suggested, { family: '', weight: 400, style: 'normal' });
  assert.equal(weightFromName('ExtraBold'), 800);
  assert.equal(weightFromName('Semi Bold'), 600);
  assert.equal(weightFromName('Ultra-Light'), 200);
  assert.equal(weightFromName('Black'), 900);
  assert.equal(weightFromName('Something'), 0);
  assert.equal(weightFromName('Notebook Sans'), 0);
  assert.equal(weightFromName('NotoSansKR-Bold'), 700);
});
