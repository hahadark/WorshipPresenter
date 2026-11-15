// 기능 대조용 자료 준비: 두 자료 폴더에 같은 시험 영상·음악·오버레이·설정을 넣는다 (두 판을 끈 상태에서)
// node tools\flow-compare\prep.js [--rs-data tmp\rs-data] [--el-data tmp\el-data]
'use strict';
const fs = require('fs'); const path = require('path'); const { execFileSync } = require('child_process');
const A = process.argv.slice(2); const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const root = path.resolve(__dirname, '..', '..');
const RS = path.resolve(root, opt('rs-data', 'tmp/rs-data')), EL = path.resolve(root, opt('el-data', 'tmp/el-data'));
const tmp = path.join(root, 'tmp', 'flow-media'); fs.mkdirSync(tmp, { recursive: true });
const mk = (name, args) => { const f = path.join(tmp, name); if (!fs.existsSync(f)) execFileSync('ffmpeg', ['-y', '-loglevel', 'error', ...args, f]); return f; };
// 멈춘 그림 같은 영상(어느 순간 찍어도 같게) · 짧은 음악 · 그림 3장
const files = [
  mk('flowtest.mp4', ['-f', 'lavfi', '-i', 'color=c=0x123a6b:s=640x360:d=30', '-f', 'lavfi', '-i', 'sine=frequency=440:duration=30', '-shortest', '-pix_fmt', 'yuv420p', '-c:v', 'libx264', '-c:a', 'aac']),
  mk('flowshort.mp4', ['-f', 'lavfi', '-i', 'color=c=0x6b1223:s=640x360:d=4', '-pix_fmt', 'yuv420p', '-c:v', 'libx264']),
  mk('flowtest.mp3', ['-f', 'lavfi', '-i', 'sine=frequency=330:duration=8', '-c:a', 'libmp3lame']),
  mk('flowimg1.png', ['-f', 'lavfi', '-i', 'color=c=0x2e7d32:s=1280x720', '-frames:v', '1']),
  mk('flowimg2.png', ['-f', 'lavfi', '-i', 'color=c=0xf9a825:s=1280x720', '-frames:v', '1']),
  mk('flowlogo.png', ['-f', 'lavfi', '-i', 'color=c=0xd32f2f:s=300x200', '-frames:v', '1']),
];
for (const d of [RS, EL]) for (const f of files) fs.copyFileSync(f, path.join(d, 'media', path.basename(f)));
const base = JSON.parse(fs.readFileSync(path.join(EL, 'settings.json'), 'utf8'));
const ov = (o) => ({ bg: '', color: '#ffffff', doneText: '', format: '24', full: false, image: '', margin: 40, on: false, opacity: 1, outputs: ['main', 'overlay'], position: 'top-right', showSeconds: true, size: 44, startedAt: 0, target: '', text: '', ...o });
base.overlays = [
  ov({ id: 'ovclock1', name: '시계', type: 'clock', showSeconds: false }),
  ov({ id: 'ovlogo1', name: '로고', type: 'logo', image: '/media/flowlogo.png', position: 'top-left', size: 140 }),
  ov({ id: 'ovcount1', name: '카운트다운', type: 'countdown', target: '10', position: 'bottom-right', size: 60, doneText: '곧 시작합니다' }),
  ov({ id: 'ovbanner1', name: '배너', type: 'banner', text: '환영합니다 · 구세군대구사랑교회', position: 'bottom-center', bg: '#b3001b', full: true, size: 40 }),
  ov({ id: 'ovtext1', name: '글자', type: 'text', text: '설교 중입니다', position: 'top-left', size: 36, outputs: ['overlay'] }),
];
Object.assign(base, { autoUpdate: false, telemetry: false, triggerKey: 'flowTestKey01', cueTimes: true });
base.output = { ...base.output, enabled: false, autoOnSend: false };
base.stagePresets = [{ id: 'sermon1', name: '설교 요점', text: '1. 은혜\n2. 믿음\n3. 순종' }];
const w = (dir, patch) => fs.writeFileSync(path.join(dir, 'settings.json'), JSON.stringify({ ...base, ...patch }, null, 1));
// 리모컨·내부망은 이 PC 안에서만 (방화벽 알림이 뜨지 않게 127.0.0.1). Electron 의 내부망 출력은 0.0.0.0 고정이라 켜지 않는다
w(RS, { port: 37530, remote: { enabled: true, pin: '246810', port: 37642, host: '127.0.0.1' }, lanOutput: { enabled: true, key: 'flowLanKey0001', port: 37652, host: '127.0.0.1' } });
w(EL, { port: 37528, remote: { enabled: true, pin: '246810', port: 37641, host: '127.0.0.1' }, lanOutput: { enabled: false, key: 'flowLanKey0001', port: 37651 } });
console.log('준비 끝:', files.map(f => path.basename(f)).join(' '));
