// 페이지(HTML): 설치판과 같은 web/ 의 파일을 그대로 가져와(복사본을 두지 않는다) 머리에 어댑터만 끼운다.
// 어댑터(client/adapter.js)가 EventSource('/events') 를 클라우드 실시간 연결로 바꿔 주므로 페이지 · 렌더러는 고치지 않는다.
import outputHtml from '../../web/output/index.html';
import stageHtml from '../../web/output/stage.html';
import remoteHtml from '../../web/remote/index.html';
import editorHtml from '../../web/editor/index.html';
import loginHtml from '../client/login.html';
import ownerHtml from '../client/owner.html';
import deniedHtml from '../client/denied.html';

const PAGES = { output: outputHtml, stage: stageHtml, remote: remoteHtml, editor: editorHtml, login: loginHtml, owner: ownerHtml, denied: deniedHtml };
/** 웹판이 새로 만든 페이지 (어댑터가 필요 없고, 엄격한 보안 머리글을 붙인다) */
const OWN = ['login', 'owner', 'denied'];

/** <head> 바로 뒤에 설정과 어댑터를 넣는다. 설정 값이 글 밖으로 새지 않게 '<' 를 바꾼다 */
export function inject(html, cfg) {
  const data = JSON.stringify(cfg).replace(/</g, '\\u003c');
  // 2C: 로그인한 페이지는 교회가 올린 글꼴(@font-face)을 받는다 — 글꼴이 없는 PC 의 OBS · 브라우저에서도 템플릿 글꼴로 보이게
  const fonts = cfg.authed ? '<link rel="stylesheet" href="/api/v1/assets/fonts.css">' : '';
  const tag = `<script>window.WP_CLOUD=${data}</script><script src="/cloud/adapter.js"></script>${fonts}`;
  return /<head[^>]*>/i.test(html) ? html.replace(/<head([^>]*)>/i, `<head$1>${tag}`) : tag + html;
}

export function pageFor(name, cfg) {
  const html = PAGES[name];
  if (!html) throw new Error(`모르는 페이지: ${name}`);
  if (OWN.includes(name)) return html.replace('"__WP_CLOUD__"', JSON.stringify(cfg).replace(/</g, '\\u003c'));
  return inject(html, cfg);
}

/**
 * 페이지 응답. 새로 만든 페이지(로그인 · 주인)는 엄격한 CSP, 리모컨 · 편집기는 다른 사이트 안에 끼워 넣지 못하게만,
 * 출력 화면은 영상 · 웹 소스 · 유튜브를 끼우므로 느슨하게 둔다 (설치판과 같음).
 */
export function page(name, cfg, status = 200) {
  const headers = {
    'content-type': 'text/html; charset=utf-8',
    'cache-control': 'no-store',
    'referrer-policy': 'no-referrer',
    'x-content-type-options': 'nosniff',
  };
  if (OWN.includes(name)) headers['content-security-policy'] = "default-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";
  if (OWN.includes(name) || name === 'remote' || name === 'editor') headers['x-frame-options'] = 'DENY';
  return new Response(pageFor(name, cfg), { status, headers });
}
