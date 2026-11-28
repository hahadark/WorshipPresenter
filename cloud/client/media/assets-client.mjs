// 2C 파일 저장(R2) 올리기 — 브라우저 쪽 작은 도우미 (계약: docs/web/assets-api.md 3절, 조율자가 정한 1~5번).
// 웹 조작 화면(web-egui) · PDF(pdf.mjs)가 같이 쓴다. node 시험에서도 돈다 (fetch · crypto.subtle 만 씀).

/** 16진 sha256 */
export async function sha256Hex(data) {
  const buf = data instanceof ArrayBuffer ? data : data.buffer ? data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) : await data.arrayBuffer();
  const d = await crypto.subtle.digest('SHA-256', buf);
  return [...new Uint8Array(d)].map(b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * 파일 하나 올리기 (90 MiB 이하 — 그보다 크면 나눠 올리기 3-3 이 필요하다: 영상은 설치판(2B) · 웹 조작 화면의 일).
 * 먼저 같은 sha256 이 있나 묻고(3-2), 없을 때만 올린다(3-1). 돌려줌: { id, url, existed, sha256 }
 */
export async function uploadAsset(blob, name, { fetch: f = globalThis.fetch, type } = {}) {
  const bytes = await blob.arrayBuffer();
  if (bytes.byteLength > 90 * 1024 * 1024) throw new Error('90 MiB 를 넘는 파일은 나눠 올리기(/api/v1/assets/uploads)로 올려야 합니다.');
  const sha256 = await sha256Hex(bytes);
  const have = await f(`/api/v1/assets?sha256=${sha256}`, { headers: { 'x-worshipcue': '1' } }).then(r => (r.ok ? r.json() : null)).catch(() => null);
  if (have && have.asset) return { id: have.asset.id, url: have.asset.url || `/api/v1/assets/${have.asset.id}`, existed: true, sha256 };
  const r = await f('/api/v1/assets', {
    method: 'POST',
    headers: { 'content-type': type || blob.type || 'application/octet-stream', 'x-worshipcue': '1', 'x-asset-sha256': sha256, 'x-asset-name': encodeURIComponent(String(name).split(/[\\/]/).pop()) },
    body: bytes,
  });
  const j = await r.json().catch(() => ({}));
  if (!r.ok || !j.ok) {
    const e = new Error(j.error || `올리지 못했습니다 (${r.status})`);
    e.status = r.status; e.quota = r.status === 507;
    throw e;
  }
  return { id: j.id, url: (j.asset && j.asset.url) || `/api/v1/assets/${j.id}`, existed: !!j.existed, sha256 };
}

/** 설치판 pdf.rs 가 만드는 큐시트 순서와 같은 모양 (슬라이드쇼 · 자동 넘김 없음 · pdf:true) */
export function pdfSlideshowItem(title, urls) {
  return { type: 'slideshow', title: String(title || 'PDF'), images: urls.slice(), body: '', autoSec: 0, autoLoop: false, countdown: '', pdf: true };
}
