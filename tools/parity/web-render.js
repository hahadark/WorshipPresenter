/* 점검: 웹 출력(render.js)으로 resolved.json 의 한 경우를 1920×1080 에 그린다.
 * 사용: tools\parity\run.ps1 로 띄운 점검 앱의 http://127.0.0.1:<port>/outputs/main 을 브라우저(1920×1080 창)로 열고,
 *       개발자 도구 콘솔에 이 파일 내용을 붙여 넣은 뒤 parityRender(<resolved.json 의 한 항목>) → 화면을 찍어 rust-<이름>.png 와 나란히 비교.
 * (조작 화면 그림과 웹 출력이 같은 규칙인지 보는 도구. 2026-11-15 docs/slide-parity.md)
 */
window.parityRender = async function (c) {
  let host = document.getElementById('parityHost');
  if (!host) {
    host = document.createElement('div');
    host.id = 'parityHost';
    host.style.cssText = 'position:fixed;left:0;top:0;width:1920px;height:1080px;z-index:99999;background:#000;overflow:hidden';
    document.body.appendChild(host);
  }
  host.innerHTML = '';
  const t = c.template;
  const r = window.BCRender.renderInto(host, c.slide && c.slide.kind ? c.slide : null, t, {
    versionName: c.versionName, checker: true,
    layers: { hideBg: c.hideBg, logo: c.logo, logoImage: (t.logo && t.logo.image) || '/shared/assets/sa-shield.png' },
  });
  if (c.pipTemplate) {
    // output/index.html .bc-pip (OBS 자막 그림 우상단)
    const el = document.createElement('div');
    el.style.cssText = `position:absolute;top:${1080 * 0.03}px;right:${1920 * 0.02}px;width:${1920 * 0.3}px;z-index:50;border-radius:19.2px;overflow:hidden;box-shadow:0 9.6px 38.4px rgba(0,0,0,.55);aspect-ratio:${c.pipTemplate.width} / ${c.pipTemplate.height}`;
    host.appendChild(el);
    window.BCRender.renderInto(el, { ...c.slide, caption: '' }, c.pipTemplate);
  }
  await document.fonts.ready;
  await Promise.all([...host.querySelectorAll('img')].map(i => (i.decode ? i.decode().catch(() => {}) : 0)));
  return r.size; // 웹 fit 크기 (점검 앱의 fit.txt 와 같아야 한다)
};
