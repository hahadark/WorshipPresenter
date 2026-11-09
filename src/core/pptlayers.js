'use strict';
// PPT 요소 따로 고치기: PowerPoint 가 꺼낸 요소 정보(pptrender.js exportLayers → layers.json) → 슬라이드 편집기의 글상자·그림
//   raw = { v, w, h (슬라이드 크기, 포인트), slides: [[요소…]] } — 요소는 슬라이드의 쌓인 차례대로 (뒤 → 앞)
//     글   { kind:'text', l, t, w, h, text, size, bold, italic, color(BGR 정수), font, fontEa,
//            align(1 왼 2 가운데 3 오른), anchor(1·2 위 3 가운데 4·5 아래), ml, mr, mt, mb, lh, wrap }
//     그림 { kind:'image', l, t, w, h, file: 'slide-001-e01.png' }   (그림·도형·표·차트를 투명 배경 PNG 로)
//   편집기 글상자 = { id:'p{k}t{n}', type:'text', x, y, w, h (화면 %), text, size (1080 줄 화면 px), color, bold, italic, align, valign, padding, font, fit, lineHeight }
//   편집기 그림   = { id:'p{k}i{n}', type:'image', x, y, w, h, src, fit:'contain' }
// 슬라이드 그림은 화면에 맞춰 가운데에 놓인다 (object-fit: contain) — 화면 비율이 달라도 글상자가 그림 위 같은 자리에 오게 맞춘다

const HANGUL = /[ㄱ-ㆎ가-힣]/;
const r2 = n => Math.round(n * 100) / 100;

/** BGR 정수(PowerPoint RGB) → #rrggbb */
function bgrHex(c) {
  const n = Number(c);
  if (!Number.isFinite(n) || n < 0 || n > 0xffffff) return '#ffffff';
  const r = n & 255, g = (n >> 8) & 255, b = (n >> 16) & 255;
  return '#' + [r, g, b].map(v => v.toString(16).padStart(2, '0')).join('');
}

/** 글꼴 이름 (화면 CSS 에 그대로 쓰니 글자·숫자·공백·몇 가지 부호만) */
function fontName(s) {
  const t = String(s || '').trim();
  return /^[\w가-힣ㄱ-ㆎ .+\-()]{1,60}$/.test(t) && !/^\+/.test(t) ? t : ''; // '+mn-ea' 같은 테마 글꼴 이름은 버림
}

/** 슬라이드 그림이 화면에서 차지하는 자리 (%) */
function fitRect(slideW, slideH, screenW, screenH) {
  const sa = slideW / slideH, ca = screenW / screenH;
  if (!(sa > 0) || !(ca > 0)) return { x: 0, y: 0, w: 100, h: 100 };
  if (sa >= ca) { const h = ca / sa * 100; return { x: 0, y: (100 - h) / 2, w: 100, h }; }
  const w = sa / ca * 100;
  return { x: (100 - w) / 2, y: 0, w, h: 100 };
}

/**
 * 슬라이드 하나의 글상자들 → 편집기 글상자 목록
 * k = 슬라이드 번호 (0부터, id 에 씀) · screen = { w, h } 송출 화면 크기
 */
function toItems(raw, k, screen = { w: 1920, h: 1080 }, urlBase = '') {
  const W = Number(raw && raw.w) || 960, H = Number(raw && raw.h) || 540;
  const list = raw && Array.isArray(raw.slides) && Array.isArray(raw.slides[k]) ? raw.slides[k] : [];
  const fr = fitRect(W, H, Number(screen.w) || 1920, Number(screen.h) || 1080);
  const pt = fr.h / 100 * 1080 / H; // 1 포인트 → 1080 줄 화면 px
  const out = [];
  const rect = b => ({
    x: r2(fr.x + Number(b.l) / W * fr.w), y: r2(fr.y + Number(b.t) / H * fr.h),
    w: r2(Math.max(1, Number(b.w) / W * fr.w)), h: r2(Math.max(1, Number(b.h) / H * fr.h)),
  });
  list.forEach((b, n) => {
    if (!b || typeof b !== 'object') return;
    if (b.kind === 'image') { // 그림·도형·표·차트 (요소 그림)
      if (!/^slide-\d{3,4}-e\d{2,3}\.png$/.test(String(b.file || '')) || !urlBase) return;
      out.push({ id: `p${k}i${n}`, type: 'image', ...rect(b), src: urlBase + b.file, fit: 'contain', opacity: 1, radius: 0, below: false });
      return;
    }
    const text = String(b.text ?? '').replace(/\r\n?|\u000b/g, '\n').replace(/\s+$/, '');
    if (!text.trim()) return;
    const font = HANGUL.test(text) ? fontName(b.fontEa) || fontName(b.font) : fontName(b.font) || fontName(b.fontEa);
    const lh = Number(b.lh) > 0 ? Math.max(0.8, Math.min(3, r2(1.2 * Number(b.lh)))) : 1.2;
    out.push({
      id: `p${k}t${n}`, type: 'text', ...rect(b),
      text: text.slice(0, 2000),
      size: Math.max(8, Math.min(400, Math.round((Number(b.size) || 18) * pt * 10) / 10)),
      color: bgrHex(b.color), bold: Number(b.bold) === -1 || b.bold === true, italic: Number(b.italic) === -1 || b.italic === true,
      align: { 1: 'left', 2: 'center', 3: 'right' }[b.align] || 'left',
      valign: { 1: 'top', 2: 'top', 3: 'middle', 4: 'bottom', 5: 'bottom' }[b.anchor] || 'top',
      padding: Math.max(0, Math.min(300, Math.round((b.ml != null && Number.isFinite(Number(b.ml)) ? Number(b.ml) : 7.2) * pt))), // 여백 0 도 그대로
      ...(font ? { font } : {}), fit: true, lineHeight: lh, opacity: 1, radius: 0, below: false,
    });
  });
  return out;
}

/** 모든 슬라이드 → [[글상자·그림…]] (urlBase = 요소 그림 주소 앞부분, 예: '/hymn/<key>/') */
function allItems(raw, screen, urlBase = '') {
  const n = raw && Array.isArray(raw.slides) ? raw.slides.length : 0;
  return Array.from({ length: n }, (_, k) => toItems(raw, k, screen, urlBase));
}

/** 편집기 글상자·그림 id 가 PPT 에서 꺼낸 것인가 (p{슬라이드}t{번호} 글 · p{슬라이드}i{번호} 그림) */
const isPptItem = id => /^p\d+[ti]\d+$/.test(String(id || ''));

module.exports = { toItems, allItems, bgrHex, fontName, fitRect, isPptItem };
