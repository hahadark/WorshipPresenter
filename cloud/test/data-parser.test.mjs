// 장절 파서 대조: 웹판 src/data/parser.mjs 가 설치판 crates/wp-core/src/parser.rs 와 같은 답을 내는가.
// 정답 test/data-golden/golden.json = Rust 코드를 그대로 돌린 결과 (test/data-golden/Cargo.toml 머리말에 만드는 법).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseReference, resolveBook, normalizeInput, looksLikeReference, formatTitle, BOOKS } from '../src/data/parser.mjs';

const golden = JSON.parse(readFileSync(new URL('./data-golden/golden.json', import.meta.url), 'utf8'));
const cases = JSON.parse(readFileSync(new URL('./data-golden/cases.json', import.meta.url), 'utf8'));

function shape(p) {
  if (p.ok) return { ok: true, book: p.book.id, ranges: p.ranges.map(r => ({ ec: r.ec, ev: r.ev, sc: r.sc, sv: r.sv })) };
  return { ok: false, error: p.error, book: p.book ? p.book.id : null, candidates: p.candidates.map(b => b.id), incomplete: p.incomplete, ambiguous: p.ambiguous, rest: p.rest };
}

test('정답 파일이 지금 입력 목록과 같다 (cases.json 을 고쳤으면 golden.json 을 다시 만들 것)', () => {
  assert.deepEqual(golden.map(g => g.input), cases);
  assert.ok(golden.length >= 100, `${golden.length}개`);
  assert.equal(BOOKS.length, 66);
});

for (const g of golden) {
  test(`설치판과 같음: ${JSON.stringify(g.input)}`, () => {
    assert.equal(normalizeInput(g.input), g.normalized, '정규화');
    assert.deepEqual(shape(parseReference(g.input)), g.parsed, '장절');
    assert.equal(looksLikeReference(g.input), g.looksLike, '장절처럼 보이나');
    const rb = resolveBook(g.input);
    assert.deepEqual({ book: rb.book ? rb.book.id : null, candidates: rb.candidates.map(b => b.id) }, g.resolveBook, '책 이름');
  });
}

test('제목 만들기 (db.rs format_title)', () => {
  assert.equal(formatTitle('요한복음', [[3, 16, 3, 18], [3, 20, 3, 20]]), '요한복음 3:16-18, 20');
  assert.equal(formatTitle('창세기', [[1, 1, 2, 3]]), '창세기 1:1-2:3');
  assert.equal(formatTitle('로마서', [[8, 1, 8, 1], [8, 28, 8, 28]]), '로마서 8:1, 28');
  assert.equal(formatTitle('시편', [[23, 1, 23, 6], [24, 1, 24, 1]]), '시편 23:1-6, 24:1');
});
