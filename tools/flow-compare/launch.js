// 점검용 두 판 띄우기/끄기 (분리 실행 — 셸이 붙잡히지 않게). 자기가 띄운 PID 만 tmp\flow-pids.json 에 적고, stop 은 그 PID 만 끈다.
// node tools\flow-compare\launch.js start [--no-el] [--rs-data tmp\rs-data] [--el-data tmp\el-data]
// node tools\flow-compare\launch.js stop
'use strict';
const { spawn, execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const root = path.resolve(__dirname, '..', '..');
const PIDS = path.join(root, 'tmp', 'flow-pids.json');
const read = () => { try { return JSON.parse(fs.readFileSync(PIDS, 'utf8')); } catch { return {}; } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const alive = pid => { try { process.kill(pid, 0); return true; } catch { return false; } };

async function main() {
  const pids = read();
  if (A[0] === 'stop') {
    for (const [k, pid] of Object.entries(pids)) { if (alive(pid)) { try { execFileSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} } console.log(`${k} ${pid} 종료`); }
    fs.writeFileSync(PIDS, '{}');
    return;
  }
  const rsData = path.resolve(root, opt('rs-data', 'tmp/rs-data'));
  const elData = path.resolve(root, opt('el-data', 'tmp/el-data'));
  const NAME = opt('name', 'rs'); // 점검 서버를 여러 개 띄울 때 이름 (PID 기록용)
  if (!pids[NAME] || !alive(pids[NAME])) {
    for (const f of ['flow-port.txt', 'flow-ctl.txt']) try { fs.unlinkSync(path.join(rsData, f)); } catch {}
    // --exe: 다른 자리의 실행 파일로 (오래 도는 일괄 대조 중에도 target 의 것을 다시 빌드할 수 있게 복사본을 쓴다)
    const p = spawn(path.resolve(root, opt('exe', 'target/debug/examples/flow_driver.exe')), ['--data', rsData, '--web', path.join(root, 'web')], { detached: true, stdio: 'ignore', windowsHide: true });
    p.unref(); pids[NAME] = p.pid;
  }
  if (NAME === 'rs' && !A.includes('--no-el') && (!pids.el || !alive(pids.el))) {
    const s = JSON.parse(fs.readFileSync(path.join(elData, 'settings.json'), 'utf8'));
    if (s.autoUpdate !== false) throw new Error('el-data settings.json 에 "autoUpdate": false 가 없습니다');
    const p = spawn('F:\\aside\\WorshipCue\\node_modules\\electron\\dist\\electron.exe', ['F:\\aside\\WorshipCue', '--data', elData], { detached: true, stdio: 'ignore' });
    p.unref(); pids.el = p.pid;
  }
  fs.mkdirSync(path.dirname(PIDS), { recursive: true });
  fs.writeFileSync(PIDS, JSON.stringify(pids));
  for (let i = 0; i < 240 && !fs.existsSync(path.join(rsData, 'flow-ctl.txt')); i++) await sleep(250);
  const out = { rs: fs.readFileSync(path.join(rsData, 'flow-port.txt'), 'utf8').trim(), pids };
  if (NAME === 'rs' && pids.el && !A.includes('--no-el')) {
    // Electron 포트: 우리가 띄운 PID 가 듣고 있는 포트 가운데 /control/ 이 200 인 것.
    // (다른 점검 갈래의 Electron 이 37528 을 쓰고 있을 수 있다 — 첫 200 을 잡으면 남의 앱을 조작하게 된다)
    const owned = () => { try { return execFileSync('netstat', ['-ano', '-p', 'TCP'], { encoding: 'utf8' }).split('\n').map(l => l.trim().split(/\s+/)).filter(c => c[3] === 'LISTENING' && Number(c[4]) === pids.el).map(c => Number(c[1].split(':').pop())); } catch { return []; } };
    for (let i = 0; i < 80 && !out.el; i++) {
      for (const p of owned().sort((x, y) => x - y)) {
        if (out.el) break;
        try { const r = await fetch(`http://127.0.0.1:${p}/control/`, { signal: AbortSignal.timeout(700) }); if (r.status === 200) out.el = String(p); } catch {}
      }
      if (!out.el) await sleep(500);
    }
    if (!out.el) throw new Error('Electron 포트를 찾지 못했습니다');
  }
  if (NAME === 'rs') fs.writeFileSync(path.join(root, 'tmp', 'flow-ports.json'), JSON.stringify(out));
  console.log(JSON.stringify(out));
}
main().catch(e => { console.error(e.message); process.exit(1); });
