// 아주 작은 PNG 읽기·쓰기 (8비트 RGB/RGBA, 비월 없음 — 크롬 스크린샷 형식). 외부 모듈 없이 zlib 만.
'use strict';
const zlib = require('zlib');

function decode(buf) {
  let p = 8, w = 0, h = 0, ct = 0, bd = 0; const idat = [];
  while (p < buf.length) {
    const len = buf.readUInt32BE(p); const type = buf.toString('ascii', p + 4, p + 8); const d = buf.subarray(p + 8, p + 8 + len);
    if (type === 'IHDR') { w = d.readUInt32BE(0); h = d.readUInt32BE(4); bd = d[8]; ct = d[9]; if (d[12]) throw new Error('비월 PNG 는 못 읽음'); }
    else if (type === 'IDAT') idat.push(d);
    else if (type === 'IEND') break;
    p += 12 + len;
  }
  if (bd !== 8 || (ct !== 2 && ct !== 6)) throw new Error(`지원하지 않는 PNG (bd=${bd} ct=${ct})`);
  const bpp = ct === 6 ? 4 : 3; const raw = zlib.inflateSync(Buffer.concat(idat)); const stride = w * bpp;
  const out = Buffer.alloc(w * h * 4); const prev = Buffer.alloc(stride); const cur = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)]; const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? cur[x - bpp] : 0, b = prev[x], c = x >= bpp ? prev[x - bpp] : 0;
      let v = line[x];
      if (f === 1) v += a; else if (f === 2) v += b; else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) { const pp = a + b - c, pa = Math.abs(pp - a), pb = Math.abs(pp - b), pc = Math.abs(pp - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      cur[x] = v & 255;
    }
    for (let x = 0; x < w; x++) { const o = (y * w + x) * 4; out[o] = cur[x * bpp]; out[o + 1] = cur[x * bpp + 1]; out[o + 2] = cur[x * bpp + 2]; out[o + 3] = bpp === 4 ? cur[x * bpp + 3] : 255; }
    cur.copy(prev);
  }
  return { w, h, data: out };
}

const CRC = (() => { const t = new Int32Array(256); for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c; } return t; })();
function crc(b) { let c = -1; for (const x of b) c = CRC[(c ^ x) & 255] ^ (c >>> 8); return (c ^ -1) >>> 0; }
function chunk(type, d) { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(type, 'ascii'), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); }

function encode({ w, h, data }) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) { raw[y * (w * 4 + 1)] = 0; data.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4); }
  const ih = Buffer.alloc(13); ih.writeUInt32BE(w, 0); ih.writeUInt32BE(h, 4); ih[8] = 8; ih[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ih), chunk('IDAT', zlib.deflateSync(raw, { level: 6 })), chunk('IEND', Buffer.alloc(0))]);
}

/** 두 그림 차이: 채널 차가 thr 보다 큰 화소 비율 + 차이 상자 */
function diff(a, b, thr = 48) {
  if (a.w !== b.w || a.h !== b.h) return { ratio: 1, box: null };
  let n = 0, x0 = 1e9, y0 = 1e9, x1 = -1, y1 = -1;
  for (let i = 0, k = 0; i < a.data.length; i += 4, k++) {
    if (Math.abs(a.data[i] - b.data[i]) > thr || Math.abs(a.data[i + 1] - b.data[i + 1]) > thr || Math.abs(a.data[i + 2] - b.data[i + 2]) > thr) {
      n++; const x = k % a.w, y = (k / a.w) | 0; if (x < x0) x0 = x; if (y < y0) y0 = y; if (x > x1) x1 = x; if (y > y1) y1 = y;
    }
  }
  return { ratio: n / (a.w * a.h), box: n ? [x0, y0, x1, y1] : null };
}

/** 나란히 (반 크기로 줄여 왼쪽 Electron · 오른쪽 Rust) */
function sideBySide(a, b) {
  const W = (a.w >> 1), H = (a.h >> 1); const out = { w: W * 2 + 8, h: H, data: Buffer.alloc((W * 2 + 8) * H * 4, 255) };
  for (const [img, ox] of [[a, 0], [b, W + 8]]) for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const s = ((y * 2) * img.w + x * 2) * 4, d = (y * out.w + ox + x) * 4; img.data.copy(out.data, d, s, s + 4);
  }
  return out;
}

module.exports = { decode, encode, diff, sideBySide };
