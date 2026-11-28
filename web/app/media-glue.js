/* 워십프리젠터 웹 조작 화면 — 미디어 붙이기 (4단계, web-scenes).
 * Rust(wasm, crates/wp-app/src/web/media.rs)가 부르는 작은 브라우저 도우미. 결과는 JSON 글로 돌려준다.
 * 4B 부품(/cloud/media/assets-client.mjs · pdf.mjs)을 그대로 쓴다 — 클라우드에서 열었을 때만 있다 (docs/web/media.md 7절).
 *  - cameras()   : 이 브라우저가 보는 카메라 [{deviceId, label, kind}] (이름을 보려면 잠깐 카메라를 켰다 끈다)
 *  - phones()    : 교회 중계실에 붙은 휴대폰 카메라 (GET /rtc/phones)
 *  - phoneLink() : 휴대폰을 카메라로 쓰는 링크 (POST /api/v1/media/phone-link) — QR 은 조작 화면이 그린다
 *  - pick(accept, multiple) : 파일 고르기 → 손잡이 번호들 (누른 직후에만 열린다 — 브라우저 규칙)
 *  - take(h) : 끌어다 놓은 파일을 손잡이로 받아 둠 (Rust 가 바이트로 넘김)
 *  - put(h, title) : 파일 하나를 큐시트 순서로 — 그림 · 영상 · 소리 · PDF(쪽 그림 슬라이드쇼) · 글꼴(교회 글꼴) · PPT(올리기만)
 */
(function () {
  'use strict';
  const files = new Map();
  let seq = 0;
  const keep = (f) => { const h = ++seq; files.set(h, f); return h; };
  const J = (v) => JSON.stringify(v);
  const cloud = () => !!(window.WP_CLOUD && window.WP_CLOUD.authed);
  const ext = (n) => (String(n).split('.').pop() || '').toLowerCase();
  const KIND = {
    image: ['png', 'jpg', 'jpeg', 'jfif', 'gif', 'webp', 'avif', 'bmp', 'svg'],
    video: ['mp4', 'm4v', 'webm', 'mov', 'ogv'],
    audio: ['mp3', 'm4a', 'aac', 'wav', 'ogg', 'oga', 'opus', 'flac'],
    pdf: ['pdf'],
    font: ['ttf', 'otf', 'ttc', 'woff', 'woff2'],
    ppt: ['pptx', 'ppsx', 'ppt', 'pps'],
  };
  const kindOf = (f) => { const e = ext(f.name); for (const k in KIND) if (KIND[k].includes(e)) return k; return ''; };
  const base = (n) => String(n).replace(/\.[^.]+$/, '');

  async function cameras() {
    try {
      let list = await navigator.mediaDevices.enumerateDevices();
      if (!list.some((d) => d.kind === 'videoinput' && d.label)) {
        // 이름을 보려면 허락이 필요하다: 잠깐 켰다 바로 끈다 (설치판 도우미 페이지와 같은 방법)
        const s = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        s.getTracks().forEach((t) => t.stop());
        list = await navigator.mediaDevices.enumerateDevices();
      }
      return J({ devices: list.filter((d) => d.kind === 'videoinput').map((d) => ({ deviceId: d.deviceId, label: d.label, kind: d.kind })) });
    } catch (e) {
      return J({ devices: [], error: String((e && e.message) || e) });
    }
  }

  async function phones() {
    if (!cloud()) return J({ phones: [] });
    try { const r = await fetch('/rtc/phones'); return J(await r.json()); } catch (e) { return J({ phones: [], error: String(e) }); }
  }

  async function phoneLink(name) {
    const r = await fetch('/api/v1/media/phone-link', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: J({ name: name || '' }) });
    return J(await r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' })));
  }

  function pick(accept, multiple) {
    return new Promise((res) => {
      const i = document.createElement('input');
      i.type = 'file'; i.accept = accept || ''; i.multiple = !!multiple; i.style.display = 'none';
      document.body.appendChild(i);
      i.addEventListener('change', () => { res(J([...i.files].map(keep))); i.remove(); });
      i.addEventListener('cancel', () => { res('[]'); i.remove(); });
      i.click();
    });
  }

  function take(name, bytes, type) { return keep(new File([bytes], name, { type: type || '' })); }

  async function upload(f) {
    const m = await import('/cloud/media/assets-client.mjs');
    return m.uploadAsset(f, f.name);
  }

  async function put(h, opts) {
    const f = files.get(h);
    files.delete(h);
    if (!f) return J({ ok: false, error: '파일이 없습니다.' });
    const k = kindOf(f);
    if (!k) return J({ ok: false, error: `${f.name}: 넣을 수 없는 종류입니다 (그림 · 영상 · 소리 · PDF · 글꼴 · PPT).` });
    if (!cloud()) return J({ ok: false, error: '파일 넣기는 교회 클라우드에 로그인했을 때 됩니다 (파일은 교회 저장소에 올라갑니다).' });
    const title = base(f.name);
    try {
      if (k === 'pdf') {
        const { pdfToSlideshow } = await import('/cloud/media/pdf.mjs');
        const onPage = (i, n) => { window.wpMediaProgress = `${f.name} ${i + 1}/${n}쪽`; };
        const { item, pages } = await pdfToSlideshow(f, { title, width: 1920, onPage });
        window.wpMediaProgress = '';
        return J({ ok: true, kind: k, item, pages });
      }
      const u = await upload(f);
      if (k === 'image') return J({ ok: true, kind: k, item: { type: 'image', title, image: u.url } });
      if (k === 'video') return J({ ok: true, kind: k, item: { type: 'video', title, file: u.url, sound: true, volume: 1, loop: false, endAction: 'hold' } });
      if (k === 'audio') return J({ ok: true, kind: k, url: u.url, title });
      if (k === 'font') return J({ ok: true, kind: k, url: u.url, title, note: `교회 글꼴 '${title}' 을 올렸습니다 — 출력 화면이 /api/v1/assets/fonts.css 로 받습니다 (이름은 주인 화면에서 고칠 수 있음).` });
      return J({ ok: true, kind: k, url: u.url, title, note: `PowerPoint '${title}' 을 교회 저장소에 올렸습니다. 웹에서는 아직 슬라이드 그림으로 만들지 못합니다 — 설치판이 만든 뒤(2B 동기화) 순서로 쓰세요.` });
    } catch (e) {
      return J({ ok: false, error: `${f.name}: ${(e && e.message) || e}` });
    }
  }

  window.wpMedia = { cameras, phones, phoneLink, pick, take, put, kindOf: (n) => kindOf({ name: n }) };
})();
