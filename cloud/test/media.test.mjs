// 4단계 B 미디어 단위 시험 (Cloudflare 없이 node): 길 · 휴대폰 링크 · 캐시 시계 맞추기 · 카메라 이름 맞추기 · 올리기 도우미 · PDF 순서 모양
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { route, FROM_VIEWER } from '../src/protocol.mjs';
import { PHONE, freshen, isPhonePath, makePhoneLink, matchDevice, mediaRoute, normLabel, okId, readPhoneLink } from '../src/media/routes.mjs';
import { viewerLimit } from '../src/media/signal.mjs';
import { pdfSlideshowItem, sha256Hex, uploadAsset } from '../client/media/assets-client.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const q = s => new URLSearchParams(s);
const SECRET = 'x'.repeat(40);

test('미디어 길: 휴대폰 목록 · 카메라 맞추기 · 휴대폰 링크, 1단계 파일 중계 길(/media/…)과 겹치지 않음', () => {
  assert.deepEqual(route('GET', '/rtc/phones', q('')), { kind: 'room', cap: 'view' });
  assert.deepEqual(route('GET', '/cameras', q('')), { kind: 'page', cap: 'view', page: 'cameras' });
  assert.deepEqual(route('POST', '/api/v1/media/phone-link', q('')), { kind: 'media-api', cap: 'write', op: 'link' });
  assert.equal(mediaRoute('GET', '/api/v1/media/phone-link'), null);
  assert.equal(route('GET', '/media/a.png', q('')).kind, 'media', '/media/… 는 1단계 설치판 파일 중계 그대로');
  assert.equal(route('GET', '/media/a.png', q('')).cap, 'view');
  assert.ok(isPhonePath('/phonecam') && isPhonePath('/rtc/phone') && !isPhonePath('/rtc/phones'));
  assert.ok(FROM_VIEWER.includes('rtc'));
  assert.equal(viewerLimit('{"t":"rtc","op":"offer"}'), PHONE.maxSdp + 2000, 'SDP 를 담는 rtc 글은 크게');
  assert.equal(viewerLimit('{"t":"ack"}'), 4096);
  assert.ok(okId('p1a2b3c4d5') && !okId('a b') && !okId('abc'));
});

test('휴대폰 링크: 서명 · 하루 만료 · 고치면 안 됨 · 다른 비밀값이면 안 됨', async () => {
  const now = 1_800_000_000_000;
  const { token, expiresAt } = await makePhoneLink(SECRET, 'sample', '강단 휴대폰', now);
  assert.equal(expiresAt, now + PHONE.linkTtlMs);
  assert.match(token, /^ph1\.[\w-]+\.[\w-]+$/);
  assert.deepEqual(await readPhoneLink(SECRET, token, now + 1000), { church: 'sample', exp: expiresAt, name: '강단 휴대폰' });
  assert.equal(await readPhoneLink(SECRET, token, expiresAt + 1), null, '하루 지나면');
  assert.equal(await readPhoneLink('y'.repeat(40), token, now), null);
  const [v, body, sig] = token.split('.');
  const other = Buffer.from(JSON.stringify({ c: 'other', e: expiresAt, n: 'x', l: '' })).toString('base64url');
  assert.equal(await readPhoneLink(SECRET, `${v}.${other}.${sig}`, now), null, '교회를 바꾸면');
  assert.equal(await readPhoneLink(SECRET, `${v}.${body}`, now), null);
  assert.equal(await readPhoneLink('short', token, now), null);
  assert.equal(await readPhoneLink(SECRET, 'x'.repeat(500), now), null);
});

test('캐시한 출력 자료를 늦게 줄 때 시계(now)를 그만큼 앞으로 — 영상 위치 · 무대 타이머가 틀리지 않게', () => {
  const p = JSON.stringify({ output: 'main', video: { key: 'k', playing: true, offset: 3, at: 1000, now: 5000 }, slide: { kind: 'video' } });
  const f = JSON.parse(freshen(p, 60_000));
  assert.equal(f.video.now, 65_000);
  assert.equal(f.video.at, 1000, '재생 시작 때는 그대로');
  // render.js syncVideo 가 보는 위치: offset + (지금 - (지금 - now) - at)/1000 → 늦게 받아도 실제 지난 시간만큼
  const target = (v, nowLocal) => v.offset + (nowLocal - (nowLocal - v.now) - v.at) / 1000;
  assert.equal(target(f.video, 999_999), 3 + (65_000 - 1000) / 1000, '1분 뒤에 붙은 화면도 1분 지난 자리');
  const s = JSON.parse(freshen(JSON.stringify({ timer: { running: true, remaining: 30_000, now: 10 } }), 2000));
  assert.equal(s.timer.now, 2010);
  const r = JSON.parse(freshen(JSON.stringify({ live: {}, stage: { timer: { now: 10 } } }), 2000));
  assert.equal(r.stage.timer.now, 2010, '리모컨 상태의 타이머도');
  assert.equal(freshen(p, 100), p, '짧으면 그대로 (풀지 않음)');
  const plain = JSON.stringify({ slide: { kind: 'text' } });
  assert.equal(freshen(plain, 60_000), plain);
  assert.equal(freshen('{깨짐 "now":', 60_000), '{깨짐 "now":');
});

test('카메라 이름 맞추기: 정확히 → 정리해서 같음 → 품음, 영상 장치만, 장치 번호 꼬리 무시', () => {
  const devs = [
    { kind: 'audioinput', label: 'USB 캡처보드', deviceId: 'mic' },
    { kind: 'videoinput', label: 'USB Capture HDMI (1bcf:2c99)', deviceId: 'cap' },
    { kind: 'videoinput', label: 'Logitech BRIO', deviceId: 'brio' },
    { kind: 'videoinput', label: '', deviceId: 'nolabel' },
  ];
  assert.equal(matchDevice('Logitech BRIO', devs).deviceId, 'brio');
  assert.equal(matchDevice('usb capture hdmi', devs).deviceId, 'cap', '장치 번호 꼬리 · 대소문자');
  assert.equal(matchDevice('USB Capture HDMI (aaaa:bbbb)', devs).deviceId, 'cap');
  assert.equal(matchDevice('BRIO', devs).deviceId, 'brio', '한쪽이 다른 쪽을 품음');
  assert.equal(matchDevice('USB 캡처보드', devs), null, '마이크는 아님');
  assert.equal(matchDevice('', devs), null);
  assert.equal(normLabel('  A   B (046d:0825) '), 'a b');
});

test('출력 화면 부품(client/media/media.js)의 이름 맞추기가 서버 쪽과 같다', () => {
  const src = readFileSync(join(here, '..', 'client', 'media', 'media.js'), 'utf8');
  const grab = name => src.slice(src.indexOf(`function ${name}(`), src.indexOf('\n  }', src.indexOf(`function ${name}(`)) + 4);
  const one = src.split('\n').find(l => l.includes('function normLabel('));
  // eslint-disable-next-line no-new-func
  const fns = new Function(`${one}\n${grab('matchDevice')}\nreturn { normLabel, matchDevice };`)();
  const devs = [{ kind: 'videoinput', label: 'USB Capture HDMI (1bcf:2c99)', deviceId: 'cap' }, { kind: 'videoinput', label: 'Logitech BRIO', deviceId: 'brio' }];
  for (const want of ['Logitech BRIO', 'usb capture hdmi', 'BRIO', 'nothing', '']) {
    assert.deepEqual(fns.matchDevice(want, devs), matchDevice(want, devs), want);
    assert.equal(fns.normLabel(want + ' (046d:0825)'), normLabel(want + ' (046d:0825)'));
  }
});

test('올리기 도우미: 있나 먼저 묻고(sha256), 없으면 2C 계약 머리글로 올림 · 경로 뗌 · 507 은 quota', async () => {
  const blob = new Blob([new Uint8Array([1, 2, 3, 4])], { type: 'image/png' });
  const sha = await sha256Hex(new Uint8Array([1, 2, 3, 4]));
  assert.equal(sha, '9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a');
  const calls = [];
  const fake = (status, body) => async (url, init = {}) => { calls.push({ url, init }); return { ok: status < 300, status, json: async () => body(url, init) }; };
  const have = await uploadAsset(blob, 'x.png', { fetch: fake(200, () => ({ ok: true, asset: { id: 'A1', url: '/api/v1/assets/A1' } })) });
  assert.deepEqual(have, { id: 'A1', url: '/api/v1/assets/A1', existed: true, sha256: sha });
  assert.equal(calls.length, 1, '있으면 올리지 않음');
  calls.length = 0;
  let n = 0;
  const two = async (url, init = {}) => { calls.push({ url, init }); n++; return n === 1 ? { ok: true, status: 200, json: async () => ({ ok: true, asset: null }) } : { ok: true, status: 201, json: async () => ({ ok: true, id: 'B2', existed: false, asset: { url: '/api/v1/assets/B2' } }) }; };
  const up = await uploadAsset(blob, 'C:\\Users\\누구\\주보\\1쪽.png', { fetch: two });
  assert.deepEqual(up, { id: 'B2', url: '/api/v1/assets/B2', existed: false, sha256: sha });
  const h = calls[1].init.headers;
  assert.equal(calls[1].url, '/api/v1/assets');
  assert.equal(h['x-asset-sha256'], sha);
  assert.equal(decodeURIComponent(h['x-asset-name']), '1쪽.png', 'PC 경로 · 사용자 이름을 보내지 않음');
  assert.equal(h['x-worshipcue'], '1');
  await assert.rejects(uploadAsset(blob, 'x.png', { fetch: async (url) => (url.includes('sha256') ? { ok: true, status: 200, json: async () => ({ ok: true, asset: null }) } : { ok: false, status: 507, json: async () => ({ ok: false, error: '무료 범위' }) }) }), e => e.quota === true);
});

test('PDF 순서: 설치판 pdf.rs 가 만드는 슬라이드쇼와 같은 모양', () => {
  assert.deepEqual(pdfSlideshowItem('주보', ['/api/v1/assets/a', '/api/v1/assets/b']), { type: 'slideshow', title: '주보', images: ['/api/v1/assets/a', '/api/v1/assets/b'], body: '', autoSec: 0, autoLoop: false, countdown: '', pdf: true });
  const rs = readFileSync(join(here, '..', '..', 'crates', 'wp-core', 'src', 'pdf.rs'), 'utf8');
  for (const k of ['"images"', '"autoSec"] = json!(0)', '"autoLoop"] = json!(false)', '"pdf"] = json!(true)', 'new_item("slideshow"']) assert.ok(rs.includes(k), `pdf.rs 에 ${k}`);
});
