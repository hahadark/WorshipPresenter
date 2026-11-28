//! [웹판 6단계] 올릴 줄 — 웹 조작 화면에서 고친 문서를 모아 두었다가 `POST /api/v1/sync`(20개씩)로 올린다.
//! 브라우저 · 화면과 무관한 계산만 둔다 (설치판 `cargo test -p wp-app` 에서도 시험: main.rs 의 `web_outbox_tests`).
//! 저장(IndexedDB)과 네트워크는 store.rs 가 한다 — 여기서는 '무엇을 저장할지(Persist)' 만 돌려준다.
//!
//! 규칙 (docs/web/offline.md 3절 · data-api.md 3절)
//! - 고친 것은 보내기 전에 줄에 먼저 쓴다(mutationId 포함). 답을 못 받았으면 같은 mutationId 로 다시 보낸다 → 서버가 `duplicate`.
//! - 아직 한 번도 보내지 않은 것(tries 0)은 같은 문서를 또 고치면 그 자리에서 바꾼다(줄이 길어지지 않게).
//!   한 번이라도 보낸 것은 바꾸지 않고 뒤에 새것을 붙인다 — baseRevision = 앞의 것 + 1 (서버가 차례대로 적용한다).
//! - 충돌은 여기서 풀지 않는다: 그 문서의 줄을 모두 빼고 `Conflict` 로 돌려준다 (store.rs 가 사본 규칙을 따른다).
//! - 다음 · 이전 같은 순간 조작은 문서가 아니라 이 줄에 들어오지 않는다.
use serde_json::{json, Value};
use std::collections::HashMap;

pub type Key = (String, String);

#[derive(Clone, Debug, PartialEq)]
pub struct Mutation {
    /// 줄 차례 (IndexedDB 열쇠)
    pub n: u64,
    pub mutation_id: String,
    pub kind: String,
    pub id: String,
    /// "put" | "delete"
    pub op: String,
    pub base_revision: i64,
    pub payload: Option<Value>,
    /// 보내 본 횟수 (0 이면 서버가 모른다 → 그 자리에서 바꿔도 된다)
    pub tries: u32,
    pub at: f64,
}

impl Mutation {
    /// IndexedDB 에 두는 모양 (= wire + n · tries · at)
    pub fn to_json(&self) -> Value {
        let mut v = self.wire();
        v["n"] = json!(self.n);
        v["tries"] = json!(self.tries);
        v["at"] = json!(self.at);
        v
    }
    pub fn from_json(v: &Value) -> Option<Mutation> {
        Some(Mutation {
            n: v["n"].as_u64()?,
            mutation_id: v["mutationId"].as_str()?.to_string(),
            kind: v["kind"].as_str()?.to_string(),
            id: v["id"].as_str()?.to_string(),
            op: v["op"].as_str()?.to_string(),
            base_revision: v["baseRevision"].as_i64().unwrap_or(0),
            payload: v.get("payload").filter(|p| !p.is_null()).cloned(),
            tries: v["tries"].as_u64().unwrap_or(0) as u32,
            at: v["at"].as_f64().unwrap_or(0.0),
        })
    }
    pub fn key(&self) -> Key {
        (self.kind.clone(), self.id.clone())
    }
    /// POST /api/v1/sync 의 한 줄
    pub fn wire(&self) -> Value {
        let mut v = json!({ "mutationId": self.mutation_id, "kind": self.kind, "id": self.id, "op": self.op, "baseRevision": self.base_revision });
        if let Some(p) = &self.payload {
            v["payload"] = p.clone();
        }
        v
    }
}

/// IndexedDB 에 남길 것
#[derive(Clone, Debug, PartialEq)]
pub enum Persist {
    PutMut(Mutation),
    DelMut(u64),
    /// 클라우드 판 (revision, 본문 — None 이면 지운 문서의 묘비)
    PutDoc(Key, i64, Option<Value>),
    DelDoc(Key),
}

/// 올리다 판이 달랐던 문서 — 사람이 고를 수 있게 사본 규칙으로 (store.rs)
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub key: Key,
    /// 이 화면의 마지막 것 (None = 이 화면에서 지움)
    pub mine: Option<Value>,
    /// 서버의 최신 (`current` 그대로, 없으면 Null)
    pub current: Value,
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub persist: Vec<Persist>,
    pub conflicts: Vec<Conflict>,
    /// (문서, 서버가 거절한 까닭)
    pub invalid: Vec<(Key, String)>,
    /// 들어간 mutationId (받기에서 다시 오면 건너뜀)
    pub applied: Vec<String>,
}

#[derive(Default, Debug)]
pub struct Book {
    /// 클라우드 판 번호 (지운 문서도 묘비 판을 기억 — 되살릴 때 If-Match)
    pub rev: HashMap<Key, i64>,
    /// 마지막으로 클라우드와 맞춘 본문 (이 화면의 DB 에서 꺼낸 모양의 글). 없으면 클라우드에 없거나 지움
    pub base: HashMap<Key, String>,
    pub queue: Vec<Mutation>,
    pub next_n: u64,
}

impl Book {
    /// 줄을 다시 읽어 들인 뒤 (차례대로)
    pub fn set_queue(&mut self, mut q: Vec<Mutation>) {
        q.sort_by_key(|m| m.n);
        self.next_n = q.iter().map(|m| m.n + 1).max().unwrap_or(1).max(self.next_n).max(1);
        self.queue = q;
    }

    pub fn pending(&self, k: &Key) -> bool {
        self.queue.iter().any(|m| &m.key() == k)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// 줄을 다 적용하면 이 화면이 가져야 할 것: Some(Some(본문)) · Some(None)=지움 · None=모름(클라우드에도 줄에도 없음)
    pub fn shadow(&self, k: &Key) -> Option<Option<String>> {
        if let Some(m) = self.queue.iter().rev().find(|m| &m.key() == k) {
            return Some(m.payload.as_ref().map(|p| p.to_string()));
        }
        self.base.get(k).map(|s| Some(s.clone()))
    }

    /// 지금 DB 의 문서들(now)과 비교해 바뀐 것을 줄에 넣는다
    pub fn diff(&mut self, now: &HashMap<Key, Value>, mut mid: impl FnMut() -> String, at: f64) -> Vec<Persist> {
        let mut out = vec![];
        let mut keys: Vec<&Key> = now.keys().collect();
        keys.sort();
        for k in keys {
            let v = &now[k];
            let s = v.to_string();
            if self.shadow(k).flatten().as_deref() != Some(s.as_str()) {
                out.extend(self.enqueue(k, Some(v.clone()), mid(), at));
            }
        }
        // 이 화면에서 지운 것: 클라우드에 있던 것 · 줄에서 만든 것 가운데 now 에 없는 것
        let mut gone: Vec<Key> = self.base.keys().cloned().collect();
        gone.extend(self.queue.iter().map(|m| m.key()));
        gone.retain(|k| !now.contains_key(k));
        gone.sort();
        gone.dedup();
        for k in gone {
            if self.shadow(&k).is_some_and(|s| s.is_some()) {
                out.extend(self.enqueue(&k, None, mid(), at));
            }
        }
        out
    }

    /// 문서 하나를 줄에 (payload None = 지우기)
    pub fn enqueue(&mut self, k: &Key, payload: Option<Value>, mid: String, at: f64) -> Vec<Persist> {
        let op = if payload.is_some() { "put" } else { "delete" };
        let idx: Vec<usize> = self.queue.iter().enumerate().filter(|(_, m)| &m.key() == k).map(|(i, _)| i).collect();
        if let Some(&last) = idx.last() {
            if self.queue[last].tries == 0 {
                // 아직 안 보낸 것 → 그 자리에서 바꾼다
                if payload.is_none() && idx.len() == 1 && self.queue[last].base_revision == 0 && !self.base.contains_key(k) && self.queue[last].op == "put" {
                    // 이 화면에서 만들고 올리기 전에 지움 → 서버는 모른다
                    let m = self.queue.remove(last);
                    return vec![Persist::DelMut(m.n)];
                }
                let m = &mut self.queue[last];
                m.op = op.into();
                m.payload = payload;
                m.at = at;
                return vec![Persist::PutMut(m.clone())];
            }
        }
        let base = match idx.last() {
            Some(&i) => self.queue[i].base_revision + 1,
            None => self.rev.get(k).copied().unwrap_or(0),
        };
        if payload.is_none() && idx.is_empty() && !self.base.contains_key(k) {
            return vec![]; // 클라우드에 없는 것을 지움 → 할 일 없음
        }
        let m = Mutation { n: self.next_n.max(1), mutation_id: mid, kind: k.0.clone(), id: k.1.clone(), op: op.into(), base_revision: base, payload, tries: 0, at };
        self.next_n = m.n + 1;
        self.queue.push(m.clone());
        vec![Persist::PutMut(m)]
    }

    /// 보낼 것 (앞에서 max 개). 보내 본 횟수를 올린다 — 보내기 전에 Persist 를 저장할 것
    pub fn take_batch(&mut self, max: usize) -> (Vec<Mutation>, Vec<Persist>) {
        let mut out = vec![];
        let mut p = vec![];
        for m in self.queue.iter_mut().take(max) {
            m.tries += 1;
            out.push(m.clone());
            p.push(Persist::PutMut(m.clone()));
        }
        (out, p)
    }

    /// 마지막으로 줄에 넣은 그 문서의 본문 (충돌 때 '이 화면의 것')
    fn latest(&self, k: &Key) -> Option<Option<Value>> {
        self.queue.iter().rev().find(|m| &m.key() == k).map(|m| m.payload.clone())
    }

    fn drop_key(&mut self, k: &Key, out: &mut Vec<Persist>) {
        let gone: Vec<u64> = self.queue.iter().filter(|m| &m.key() == k).map(|m| m.n).collect();
        self.queue.retain(|m| &m.key() != k);
        out.extend(gone.into_iter().map(Persist::DelMut));
    }

    /// POST /api/v1/sync 답의 results 를 적용한다
    pub fn apply_results(&mut self, batch: &[Mutation], results: &Value) -> Outcome {
        let mut o = Outcome::default();
        let by: HashMap<String, &Value> = results.as_array().into_iter().flatten().filter_map(|r| Some((r["mutationId"].as_str()?.to_string(), r))).collect();
        let mut conflicted: Vec<Key> = vec![];
        for m in batch {
            let k = m.key();
            if conflicted.contains(&k) || !self.queue.iter().any(|q| q.n == m.n) {
                continue;
            }
            let Some(r) = by.get(&m.mutation_id) else { continue }; // 답에 없음 → 다음에 다시
            match r["status"].as_str().unwrap_or("") {
                "applied" | "duplicate" => {
                    self.queue.retain(|q| q.n != m.n);
                    o.persist.push(Persist::DelMut(m.n));
                    let rev = r["revision"].as_i64().unwrap_or(m.base_revision + 1);
                    self.rev.insert(k.clone(), rev);
                    match &m.payload {
                        Some(p) => {
                            self.base.insert(k.clone(), p.to_string());
                            o.persist.push(Persist::PutDoc(k.clone(), rev, Some(p.clone())));
                        }
                        None => {
                            self.base.remove(&k);
                            o.persist.push(Persist::PutDoc(k.clone(), rev, None));
                        }
                    }
                    o.applied.push(m.mutation_id.clone());
                }
                "conflict" => {
                    let mine = self.latest(&k).unwrap_or(m.payload.clone());
                    self.drop_key(&k, &mut o.persist);
                    conflicted.push(k.clone());
                    o.conflicts.push(Conflict { key: k, mine, current: r["current"].clone() });
                }
                "missing" => {
                    // 지우려던 것이 서버에 없음 → 지운 것으로 본다
                    self.queue.retain(|q| q.n != m.n);
                    o.persist.push(Persist::DelMut(m.n));
                    self.base.remove(&k);
                    self.rev.remove(&k);
                    o.persist.push(Persist::DelDoc(k));
                }
                _ => {
                    // invalid: 같은 것을 다시 보내지 않는다 (그 본문을 맞춘 것으로 적어 둠 — 판은 그대로라 다음 고침은 맞는 판으로 간다)
                    self.queue.retain(|q| q.n != m.n);
                    o.persist.push(Persist::DelMut(m.n));
                    if let Some(p) = &m.payload {
                        if !self.pending(&k) {
                            self.base.insert(k.clone(), p.to_string());
                        }
                    }
                    o.invalid.push((k, r["error"].as_str().unwrap_or("서버가 받지 않았습니다").to_string()));
                }
            }
        }
        o
    }

    /// 클라우드 판을 받아 들였다 (받기 · 충돌 뒤 최신). text = 이 화면 DB 에 넣은 뒤 꺼낸 모양
    pub fn accept(&mut self, k: &Key, rev: i64, text: Option<String>) {
        self.rev.insert(k.clone(), rev);
        match text {
            Some(t) => {
                self.base.insert(k.clone(), t);
            }
            None => {
                self.base.remove(k);
            }
        }
    }

    /// 클라우드에 아예 없다고 들었다 (current 가 null)
    pub fn forget(&mut self, k: &Key) {
        self.rev.remove(k);
        self.base.remove(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(id: &str) -> Key {
        ("cue".into(), id.into())
    }
    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("m{n}")
        }
    }
    fn now(list: &[(&str, Value)]) -> HashMap<Key, Value> {
        list.iter().map(|(i, v)| (k(i), v.clone())).collect()
    }

    #[test]
    fn diff_puts_changed_and_new_docs_only() {
        let mut b = Book::default();
        b.accept(&k("a"), 3, Some(json!({"id":"a","name":"A"}).to_string()));
        let p = b.diff(&now(&[("a", json!({"id":"a","name":"A"})), ("b", json!({"id":"b"}))]), ids(), 1.0);
        assert_eq!(p.len(), 1);
        assert_eq!(b.queue.len(), 1);
        assert_eq!(b.queue[0].id, "b");
        assert_eq!(b.queue[0].base_revision, 0);
        // 그대로면 아무것도 안 함
        assert!(b.diff(&now(&[("a", json!({"id":"a","name":"A"})), ("b", json!({"id":"b"}))]), ids(), 2.0).is_empty());
    }

    #[test]
    fn unsent_entry_is_replaced_in_place_sent_one_gets_chained() {
        let mut b = Book::default();
        b.accept(&k("a"), 3, Some("{}".into()));
        b.diff(&now(&[("a", json!({"v":1}))]), ids(), 1.0);
        b.diff(&now(&[("a", json!({"v":2}))]), ids(), 2.0);
        assert_eq!(b.queue.len(), 1, "보내기 전이면 그 자리에서");
        assert_eq!(b.queue[0].payload, Some(json!({"v":2})));
        assert_eq!(b.queue[0].base_revision, 3);
        let mid0 = b.queue[0].mutation_id.clone();
        let (batch, _) = b.take_batch(20);
        assert_eq!(batch[0].tries, 1);
        // 답을 못 받은 사이에 또 고침 → 뒤에 붙고, 판은 앞의 것 + 1
        b.enqueue(&k("a"), Some(json!({"v":3})), "x".into(), 3.0);
        assert_eq!(b.queue.len(), 2);
        assert_eq!(b.queue[1].base_revision, 4);
        assert_eq!(b.queue[0].mutation_id, mid0, "보낸 것은 mutationId 를 바꾸지 않는다");
        assert_eq!(b.shadow(&k("a")), Some(Some(json!({"v":3}).to_string())));
    }

    #[test]
    fn delete_of_never_sent_new_doc_just_drops_it() {
        let mut b = Book::default();
        b.diff(&now(&[("n", json!({"id":"n"}))]), ids(), 1.0);
        assert_eq!(b.queue.len(), 1);
        let p = b.diff(&now(&[]), ids(), 2.0);
        assert_eq!(p, vec![Persist::DelMut(1)]);
        assert!(b.queue.is_empty());
    }

    #[test]
    fn delete_of_cloud_doc_uses_its_revision() {
        let mut b = Book::default();
        b.accept(&k("a"), 7, Some("{}".into()));
        b.diff(&now(&[]), ids(), 1.0);
        assert_eq!(b.queue[0].op, "delete");
        assert_eq!(b.queue[0].base_revision, 7);
        assert_eq!(b.queue[0].wire()["baseRevision"], 7);
        assert!(b.queue[0].wire().get("payload").is_none());
    }

    #[test]
    fn results_applied_duplicate_conflict_missing_invalid() {
        let mut b = Book::default();
        for (i, r) in [("a", 1), ("c", 2), ("d", 5), ("e", 1)] {
            b.accept(&k(i), r, Some("{}".into()));
        }
        b.enqueue(&k("a"), Some(json!({"v":"a"})), "ma".into(), 0.0);
        b.enqueue(&k("b"), Some(json!({"v":"b"})), "mb".into(), 0.0);
        b.enqueue(&k("c"), Some(json!({"v":"c1"})), "mc".into(), 0.0);
        let (batch, _) = b.take_batch(20);
        b.enqueue(&k("c"), Some(json!({"v":"c2"})), "mc2".into(), 0.0); // 앞의 것을 보낸 뒤 또 고침
        b.enqueue(&k("d"), None, "md".into(), 0.0);
        b.enqueue(&k("e"), Some(json!({"bad":true})), "me".into(), 0.0);
        let (batch2, _) = b.take_batch(20);
        assert_eq!(batch2.len(), 6);
        assert_eq!(batch.len(), 3);
        let res = json!([
            {"mutationId":"ma","status":"applied","revision":2},
            {"mutationId":"mb","status":"duplicate","revision":1},
            {"mutationId":"mc","status":"conflict","current":{"revision":9,"payload":{"v":"server"}}},
            {"mutationId":"mc2","status":"conflict","current":{"revision":9}},
            {"mutationId":"md","status":"missing"},
            {"mutationId":"me","status":"invalid","error":"너무 큼"}
        ]);
        let o = b.apply_results(&batch2, &res);
        assert!(b.queue.is_empty(), "{:?}", b.queue);
        assert_eq!(b.rev[&k("a")], 2);
        assert_eq!(b.base[&k("a")], json!({"v":"a"}).to_string());
        assert_eq!(b.rev[&k("b")], 1);
        assert_eq!(o.conflicts.len(), 1, "같은 문서의 충돌은 한 번만");
        assert_eq!(o.conflicts[0].mine, Some(json!({"v":"c2"})), "충돌 때 이 화면의 것 = 마지막에 고친 것");
        assert!(!b.rev.contains_key(&k("d")));
        assert_eq!(o.invalid.len(), 1);
        assert_eq!(o.applied, vec!["ma".to_string(), "mb".to_string()]);
        // 거절된 본문은 다시 보내지 않는다
        assert!(b.diff(&now(&[("a", json!({"v":"a"})), ("b", json!({"v":"b"})), ("e", json!({"bad":true}))]), ids(), 0.0).iter().all(|p| !matches!(p, Persist::PutMut(m) if m.id == "e")));
    }

    #[test]
    fn missing_answer_keeps_entry_for_retry_with_same_id() {
        let mut b = Book::default();
        b.enqueue(&k("a"), Some(json!({"v":1})), "ma".into(), 0.0);
        let (batch, _) = b.take_batch(20);
        let o = b.apply_results(&batch, &json!([]));
        assert!(o.persist.is_empty());
        let (again, _) = b.take_batch(20);
        assert_eq!(again[0].mutation_id, "ma");
        assert_eq!(again[0].tries, 2);
    }

    #[test]
    fn batch_is_twenty_at_most_and_in_order() {
        let mut b = Book::default();
        for i in 0..45 {
            b.enqueue(&k(&format!("d{i:02}")), Some(json!({"i":i})), format!("m{i}"), 0.0);
        }
        let (x, _) = b.take_batch(20);
        assert_eq!(x.len(), 20);
        assert_eq!(x[0].id, "d00");
        assert_eq!(x[19].id, "d19");
        let res: Vec<Value> = x.iter().map(|m| json!({"mutationId": m.mutation_id, "status":"applied","revision":1})).collect();
        b.apply_results(&x, &json!(res));
        assert_eq!(b.len(), 25);
        assert_eq!(b.take_batch(20).0[0].id, "d20");
    }

    #[test]
    fn reload_keeps_order_and_numbers() {
        let mut b = Book::default();
        b.enqueue(&k("a"), Some(json!(1)), "m1".into(), 0.0);
        b.enqueue(&k("b"), Some(json!(2)), "m2".into(), 0.0);
        let saved: Vec<Mutation> = b.queue.iter().rev().cloned().collect();
        let mut c = Book::default();
        c.set_queue(saved);
        assert_eq!(c.queue[0].id, "a");
        let p = c.enqueue(&k("z"), Some(json!(3)), "m3".into(), 0.0);
        assert!(matches!(&p[0], Persist::PutMut(m) if m.n == 3));
        // 줄 모양 그대로 저장 · 읽기
        let s = c.queue[0].to_json().to_string();
        assert!(s.contains("\"mutationId\":\"m1\"") && s.contains("\"baseRevision\":0"));
        assert_eq!(Mutation::from_json(&serde_json::from_str(&s).unwrap()).unwrap(), c.queue[0]);
    }

    #[test]
    fn deleted_on_server_then_edited_here_revives_with_tombstone_revision() {
        let mut b = Book::default();
        b.accept(&k("a"), 4, None); // 받기: 서버에서 지움 (묘비 판 4)
        b.diff(&now(&[("a", json!({"v":"mine"}))]), ids(), 0.0);
        assert_eq!(b.queue[0].base_revision, 4, "묘비 판으로 PUT = 되살리기");
    }
}
