// 휴대폰 카메라 끝까지 점검: '휴대폰' 대신 가짜 카메라를 켠 엣지로 https 카메라 화면을 열고,
// 다른 엣지에서 출력 화면(/outputs/main)이 그 영상을 WebRTC 로 받아 실제 그림 크기가 나오는지 본다.
// 준비: 점검 앱을 휴대폰 카메라 켠 설정으로 띄워 둔다 (phoneCam { enabled, port, key }).
// 사용: node tools/phonecam/e2e.js --key <열쇠> [--port 37560] [--app 37528] [--shot 결과.png]
'use strict';
const os = require('os');
const path = require('path');
const { launch, sleep } = require('../flow-compare/cdp');
const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const KEY = arg('--key');
const PORT = arg('--port', '37560');
const APP = arg('--app', '37528');
const SHOT = arg('--shot');
if (!KEY) throw new Error('--key 가 필요합니다');
const lanIp = Object.values(os.networkInterfaces()).flat().find(a => a && a.family === 'IPv4' && !a.internal && !a.address.startsWith('169.254.'))?.address;
if (!lanIp) throw new Error('내부망 주소가 없습니다');

(async () => {
  const tmp = path.join(os.tmpdir(), 'wp-phonecam-e2e-' + Date.now());
  // '휴대폰': 가짜 카메라 · 카메라 허용 창 없이 · 스스로 만든 인증서 허용
  const phoneB = await launch(path.join(tmp, 'phone'), undefined, ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--ignore-certificate-errors']);
  const viewB = await launch(path.join(tmp, 'view'));
  const out = { lanIp };
  try {
    const phone = await phoneB.open(`https://${lanIp}:${PORT}/cam?k=${KEY}`, 412, 860);
    out.phoneConnected = await phone.waitFor(`document.getElementById('dot').className.includes('ready') || document.getElementById('dot').className.includes('on')`, 20000);
    out.phoneStatus = await phone.eval(`document.getElementById('st').textContent`);
    const id = await phone.eval(`localStorage.getItem('wp-cam-id')`);
    out.phoneId = id;
    const list = await (await fetch(`http://127.0.0.1:${APP}/phonecam/list`)).json();
    out.listed = list.phones.map(p => `${p.name}@${p.ip}:${p.online}`);
    const view = await viewB.open(`http://127.0.0.1:${APP}/outputs/main`, 1280, 720);
    await sleep(1500);
    out.received = await view.eval(`(async () => {
      const s = await BCRender.cameraStream('phone:${id}');
      const v = document.createElement('video'); v.muted = true; v.autoplay = true; v.playsInline = true; v.srcObject = s;
      v.style.cssText = 'position:fixed;inset:0;width:100%;height:100%;object-fit:contain;z-index:99999;background:#000';
      document.body.appendChild(v);
      for (let i = 0; i < 80; i++) { await new Promise(r => setTimeout(r, 250)); if (v.videoWidth > 0 && v.currentTime > 0.5) return v.videoWidth + 'x' + v.videoHeight; }
      return 'none (tracks ' + s.getTracks().length + ')';
    })()`, 30000);
    await sleep(1500);
    out.phoneAfter = await phone.eval(`document.getElementById('st').textContent`);
    if (SHOT) await view.shot(SHOT);
    // 닫으면 휴대폰 쪽 연결도 정리되는지
    out.closed = await view.eval(`(BCRender.closeCamera ? (BCRender.closeCamera('phone:${id}'), 'closed') : 'no closeCamera')`);
    await sleep(2500);
    out.phoneAfterClose = await phone.eval(`document.getElementById('st').textContent`);
  } catch (e) {
    out.error = String(e && e.stack || e);
  } finally {
    phoneB.close && phoneB.close();
    viewB.close && viewB.close();
    try { phoneB.proc.kill(); viewB.proc.kill(); } catch { /* 무시 */ }
  }
  console.log(JSON.stringify(out, null, 2));
  process.exit(out.received && /^\d+x\d+$/.test(out.received) ? 0 : 1);
})();
