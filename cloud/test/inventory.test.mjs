// 기능 대조표에 메뉴 · 설정 항목이 빠짐없이 있는가 (메뉴가 바뀌면 여기서 걸린다 — 표를 함께 고칠 것)
import test from 'node:test';
import assert from 'node:assert/strict';
import { menuIds, settingIds, missing } from '../scripts/inventory.mjs';

test('기능 대조표: 메뉴 · 설정 전부', () => {
  const m = menuIds(), s = settingIds();
  assert.ok(m.length >= 60 && s.length >= 50, `메뉴 ${m.length} · 설정 ${s.length} — 원본을 못 읽음`);
  assert.deepEqual(missing([...m, ...s]), []);
});
