// PDF → 쪽 그림 → (R2 에 올려) 슬라이드쇼 순서. 설치판 pdf.rs(Windows.Data.Pdf · PDFKit)를 웹에서는 pdf.js 로.
// pdf.js 는 고정판을 이 사이트에서 내보낸다 (/cloud/vendor/pdfjs/, package.json pdfjs-dist 6.5.438 · Apache-2.0) — 예배 중 외부 CDN 을 부르지 않는다.
// 웹 조작 화면(web-egui)이 부르는 꼴:
//   import { pdfToSlideshow } from '/cloud/media/pdf.mjs';
//   const { item, pages } = await pdfToSlideshow(file, { title: '주보', width: 1920 });   // item 을 큐시트에 넣는다
// 계약: docs/web/media.md 4절
import { uploadAsset, pdfSlideshowItem } from './assets-client.mjs';

let lib = null;
async function pdfjs() {
  if (!lib) {
    lib = await import('/cloud/vendor/pdfjs/pdf.min.mjs');
    lib.GlobalWorkerOptions.workerSrc = '/cloud/vendor/pdfjs/pdf.worker.min.mjs';
  }
  return lib;
}

/** 한 번에 그릴 쪽 수 (설교 자료 · 주보면 충분, 너무 큰 PDF 로 탭이 멈추지 않게) */
export const MAX_PAGES = 200;

/**
 * PDF(File · Blob · ArrayBuffer · 같은 사이트 주소) → 쪽마다 PNG.
 * width: 그림 가로 픽셀 (기본 1920, 예배당 화면 크기). 돌려줌: [{ page, width, height, blob }]
 */
export async function renderPages(src, { width = 1920, maxPages = MAX_PAGES, onPage } = {}) {
  const { getDocument } = await pdfjs();
  const data = typeof src === 'string' ? await fetch(src).then(r => { if (!r.ok) throw new Error(`PDF 를 받지 못했습니다 (${r.status})`); return r.arrayBuffer(); })
    : src instanceof ArrayBuffer ? src : await src.arrayBuffer();
  // isEvalSupported:false — 웹 조작 화면의 CSP 에 unsafe-eval 이 없어도 되게
  const v = '/cloud/vendor/pdfjs/';
  const task = getDocument({
    data: new Uint8Array(data), isEvalSupported: false,
    cMapUrl: v + 'cmaps/', cMapPacked: true, standardFontDataUrl: v + 'standard_fonts/', wasmUrl: v + 'wasm/', iccUrl: v + 'iccs/', // 한글 CID 글꼴 · 기본 글꼴 · 그림 풀이 (모두 이 사이트에서)
  });
  const doc = await task.promise;
  const n = Math.min(doc.numPages, maxPages);
  const out = [];
  for (let i = 1; i <= n; i++) {
    const page = await doc.getPage(i);
    const base = page.getViewport({ scale: 1 });
    const vp = page.getViewport({ scale: width / base.width });
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(vp.width);
    canvas.height = Math.round(vp.height);
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#fff'; // 투명한 PDF 도 흰 종이로 (설치판과 같게)
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    await page.render({ canvasContext: ctx, canvas, viewport: vp }).promise;
    const blob = await new Promise(res => canvas.toBlob(res, 'image/png'));
    out.push({ page: i, width: canvas.width, height: canvas.height, blob });
    page.cleanup();
    if (onPage) onPage(i, n);
  }
  await task.destroy();
  return out;
}

/**
 * PDF → 쪽 그림을 R2(2C)에 올리고 → 슬라이드쇼 순서. upload 를 바꿔 끼우면(시험 · 오프라인) 그것으로 올린다.
 * 돌려줌: { item, pages: [{ page, width, height, url, existed }] }
 */
export async function pdfToSlideshow(src, { title = 'PDF', width = 1920, maxPages = MAX_PAGES, upload = uploadAsset, onPage } = {}) {
  const pages = await renderPages(src, { width, maxPages, onPage });
  const done = [];
  for (const p of pages) {
    const r = await upload(p.blob, `${title}-${String(p.page).padStart(3, '0')}.png`, { type: 'image/png' });
    done.push({ page: p.page, width: p.width, height: p.height, url: r.url, existed: !!r.existed });
  }
  return { item: pdfSlideshowItem(title, done.map(p => p.url)), pages: done };
}
