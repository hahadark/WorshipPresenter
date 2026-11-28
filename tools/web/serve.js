// [웹판] 개발용 정적 서버: node tools/web/serve.js [폴더=dist-web] [포트=37610]
// 배포는 Cloudflare Pages 가 한다 (다른 작업자 몫). 이것은 PC 에서 띄워 보는 용도.
const http = require('http'), fs = require('fs'), path = require('path'), zlib = require('zlib');
const root = path.resolve(process.argv[2] || 'dist-web'), port = +(process.argv[3] || 37610);
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml', '.ttf': 'font/ttf', '.woff2': 'font/woff2', '.db': 'application/octet-stream' };
http.createServer((req, res) => {
  let p = decodeURIComponent(req.url.split('?')[0]);
  if (p.endsWith('/')) p += 'index.html';
  let f = path.join(root, p);
  if (!f.startsWith(root)) { res.writeHead(403).end(); return; }
  // Cloudflare Pages 처럼: /outputs/main → /outputs/main.html
  if (!fs.existsSync(f) && fs.existsSync(f + '.html')) f += '.html';
  if (!fs.existsSync(f) || fs.statSync(f).isDirectory()) { res.writeHead(404).end('not found'); return; }
  const h = { 'content-type': types[path.extname(f)] || 'application/octet-stream', 'cache-control': 'no-cache' };
  // 미리 눌러 둔 파일(.br)이 있으면 그것을 (Cloudflare 가 하는 압축과 같은 조건으로 여는 시간을 재려고)
  const br = f + '.br';
  if (/\bbr\b/.test(req.headers['accept-encoding'] || '') && fs.existsSync(br)) {
    h['content-encoding'] = 'br'; h['x-wasm-size'] = String(fs.statSync(f).size);
    res.writeHead(200, h); fs.createReadStream(br).pipe(res); return;
  }
  h['content-length'] = fs.statSync(f).size;
  res.writeHead(200, h); fs.createReadStream(f).pipe(res);
}).listen(port, '127.0.0.1', () => console.log(`http://127.0.0.1:${port}/  (${root})`));
