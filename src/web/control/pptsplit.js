/* 한 PPT 를 여러 순서로 나누기 — 조작 화면 쪽 도우미 (나누기 창은 cue.js openSplit)
 *  - texts(all, meta): 송출 슬라이드마다 글자 (PowerPoint 로 꺼낸 글상자 글 · 가사 · 글 슬라이드)
 *  - sheets(all, texts): 모아보기 그림 — 슬라이드 16(또는 25)장을 번호와 함께 한 장에 (글자가 없는 PPT 를 AI 가 보게)
 *  - KIND: AI·규칙이 붙인 순서 종류 → 아이콘·이름
 */
(function () {
  'use strict';
  const KIND = {
    hymn: ['music-2', '찬송가'], song: ['music', '찬양'], bible: ['book-open', '성경'], reading: ['book', '교독문'], prayer: ['hand', '기도'],
    sermon: ['mic', '설교·강의'], notice: ['megaphone', '광고·소식'], offering: ['square-stack', '봉헌'], video: ['clapperboard', '영상'], title: ['tag', '제목·그 밖'],
  };
  const strip = t => (window.BCGroup ? window.BCGroup.stripFmt(t) : String(t ?? ''));

  /** 슬라이드마다 글자 (k = 송출 슬라이드 번호 - 1) */
  function texts(all, meta) {
    const own = meta && meta.slides && meta.slides.length === all.length ? meta.slideTexts || [] : null; // PowerPoint 로 꺼낸 글상자 글
    return all.map((s, k) => {
      if (own) return String(own[k] || '');
      if (s.caption) return String(s.caption);
      return (s.parts || []).map(p => strip(p.text)).filter(Boolean).join('\n') || String(s.subtitle || '');
    });
  }

  const load = src => new Promise(res => { const im = new Image(); im.onload = () => res(im); im.onerror = () => res(null); im.src = src; });

  /**
   * 모아보기 그림 [{ from, to, url }] — 칸마다 왼쪽 위에 노란 번호 (AI 가 슬라이드 번호를 읽는다)
   * 모두 합쳐 약 6MB 를 넘지 않게 (명령 한 번에 보낼 수 있는 크기) 화질을 낮춘다
   */
  async function sheets(all, txt, { onProgress } = {}) {
    const n = all.length;
    const cols = n > 160 ? 5 : 4;
    const per = cols * cols;
    const tw = n > 160 ? 300 : 384, th = Math.round(tw * 9 / 16);
    const canvases = [];
    for (let s = 0; s < n; s += per) {
      const cnt = Math.min(per, n - s);
      const cv = document.createElement('canvas');
      cv.width = cols * tw; cv.height = Math.ceil(cnt / cols) * th;
      const g = cv.getContext('2d');
      g.fillStyle = '#444'; g.fillRect(0, 0, cv.width, cv.height);
      for (let j = 0; j < cnt; j++) {
        const k = s + j, x = (j % cols) * tw, y = Math.floor(j / cols) * th, sl = all[k];
        g.fillStyle = sl.kind === 'score' ? '#fff' : '#000';
        g.fillRect(x + 1, y + 1, tw - 2, th - 2);
        const im = sl.image && sl.kind !== 'video' ? await load(sl.image) : null;
        if (im && im.width) {
          const r = Math.min((tw - 2) / im.width, (th - 2) / im.height), w = im.width * r, h = im.height * r;
          try { g.drawImage(im, x + 1 + (tw - 2 - w) / 2, y + 1 + (th - 2 - h) / 2, w, h); } catch { /* 그리지 못한 그림 */ }
        } else {
          // 그림이 없는 슬라이드: 글자를 작게
          g.fillStyle = '#ddd'; g.font = '15px sans-serif'; g.textBaseline = 'top';
          const lines = String(txt[k] || (sl.kind === 'video' ? '(영상)' : '')).split('\n').filter(Boolean).slice(0, 7);
          lines.forEach((l, z) => g.fillText(l.slice(0, 26), x + 10, y + 36 + z * 19, tw - 20));
        }
        const lab = String(k + 1);
        g.font = 'bold 22px sans-serif'; g.textBaseline = 'alphabetic';
        const w = g.measureText(lab).width + 12;
        g.fillStyle = '#ffd400'; g.fillRect(x + 2, y + 2, w, 28);
        g.fillStyle = '#000'; g.fillText(lab, x + 8, y + 24);
        if (onProgress) onProgress(k + 1, n);
      }
      canvases.push({ from: s + 1, to: s + cnt, cv });
    }
    let out = [];
    for (const q of [0.72, 0.55, 0.4]) {
      out = canvases.map(c => ({ from: c.from, to: c.to, url: c.cv.toDataURL('image/jpeg', q) }));
      if (out.reduce((a, x) => a + x.url.length, 0) < 6.2e6) break;
    }
    return out;
  }

  window.BCPptSplit = { texts, sheets, KIND, kindIcon: k => (KIND[k] || KIND.title)[0], kindName: k => (KIND[k] || KIND.title)[1] };
})();
