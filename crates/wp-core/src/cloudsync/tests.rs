//! 클라우드 동기화 단위 시험: 메모리 안의 가짜 클라우드 (cloud/src/data/sync.mjs · documents.mjs 와 같은 규칙 — 판 확인 · seq · mutationId 한 번만)

use super::*;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct Doc {
    rev: i64,
    payload: Option<Value>,
    deleted: bool,
}

#[derive(Default)]
struct Fake {
    docs: RefCell<BTreeMap<(String, String), Doc>>,
    changes: RefCell<Vec<Value>>,
    mids: RefCell<HashMap<String, (String, String, i64, i64)>>,
    assets: RefCell<HashMap<String, (String, Vec<u8>, String)>>,
    /// 그림·영상 길이 아직 없는 클라우드
    no_assets: Cell<bool>,
    /// 인터넷 끊김
    offline: Cell<bool>,
    /// 다음 올리기는 받아 넣고 답만 잃어버림
    lose_reply: Cell<bool>,
    /// 다음 받기에서 변경을 안 보여 줌 (그 사이에 남이 고친 경우 흉내)
    hide_once: Cell<bool>,
    pushes: Cell<usize>,
    uploads: Cell<usize>,
}

impl Fake {
    fn seq(&self) -> i64 {
        self.changes.borrow().len() as i64
    }
    fn log(&self, kind: &str, id: &str, rev: i64, op: &str, mid: Option<&str>) -> i64 {
        let seq = self.seq() + 1;
        self.changes.borrow_mut().push(json!({ "seq": seq, "kind": kind, "id": id, "revision": rev, "op": op, "mutationId": mid }));
        seq
    }
    /// 웹에서 고친 것 흉내
    fn web_put(&self, kind: &str, id: &str, payload: Value) -> i64 {
        let k = (kind.to_string(), id.to_string());
        let rev = self.docs.borrow().get(&k).map(|d| d.rev).unwrap_or(0) + 1;
        self.docs.borrow_mut().insert(k, Doc { rev, payload: Some(payload), deleted: false });
        self.log(kind, id, rev, "put", None);
        rev
    }
    fn web_delete(&self, kind: &str, id: &str) {
        let k = (kind.to_string(), id.to_string());
        let rev = self.docs.borrow().get(&k).map(|d| d.rev).unwrap() + 1;
        self.docs.borrow_mut().insert(k, Doc { rev, payload: None, deleted: true });
        self.log(kind, id, rev, "delete", None);
    }
    fn get(&self, kind: &str, id: &str) -> Option<Doc> {
        self.docs.borrow().get(&(kind.to_string(), id.to_string())).cloned()
    }
    fn live(&self, kind: &str) -> Vec<(String, Value)> {
        self.docs.borrow().iter().filter(|(k, d)| k.0 == kind && !d.deleted).map(|(k, d)| (k.1.clone(), d.payload.clone().unwrap())).collect()
    }
    fn all_text(&self) -> String {
        self.docs.borrow().values().filter_map(|d| d.payload.as_ref().map(|p| p.to_string())).collect::<Vec<_>>().join("\n")
    }
    fn current(&self, kind: &str, id: &str) -> Value {
        match self.get(kind, id) {
            None => Value::Null,
            Some(d) => json!({ "kind": kind, "id": id, "revision": d.rev, "deleted": d.deleted, "payload": d.payload }),
        }
    }
}

impl Api for Fake {
    fn pull(&self, after: i64) -> Result<Value, ApiErr> {
        if self.offline.get() {
            return Err(ApiErr::Net("끊김".into()));
        }
        let head = self.seq();
        if self.hide_once.replace(false) {
            return Ok(json!({ "ok": true, "changes": [], "next": after, "more": false, "head": after, "reset": false }));
        }
        let all: Vec<Value> = self.changes.borrow().iter().filter(|c| c["seq"].as_i64().unwrap() > after).cloned().collect();
        let page: Vec<Value> = all.iter().take(3).cloned().collect(); // 쪽 나누기도 시험 (작은 쪽)
        let out: Vec<Value> = page
            .iter()
            .map(|c| {
                let mut c = c.clone();
                let d = self.get(c["kind"].as_str().unwrap(), c["id"].as_str().unwrap()).unwrap();
                if d.rev != c["revision"].as_i64().unwrap() {
                    c["superseded"] = json!(true);
                } else if c["op"] == "put" && !d.deleted {
                    c["payload"] = d.payload.clone().unwrap();
                }
                c
            })
            .collect();
        let next = out.last().map(|c| c["seq"].as_i64().unwrap()).unwrap_or(after);
        Ok(json!({ "ok": true, "changes": out, "next": next, "more": all.len() > page.len(), "head": head, "reset": false }))
    }

    fn push(&self, mutations: &[Value]) -> Result<Value, ApiErr> {
        if self.offline.get() {
            return Err(ApiErr::Net("끊김".into()));
        }
        assert!(mutations.len() <= 20, "한 번에 20개까지");
        self.pushes.set(self.pushes.get() + 1);
        let mut results = vec![];
        for m in mutations {
            let mid = m["mutationId"].as_str().unwrap().to_string();
            assert!((8..=80).contains(&mid.len()) && mid.chars().all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c)), "mutationId 모양 {mid}");
            let (kind, id, op) = (m["kind"].as_str().unwrap().to_string(), m["id"].as_str().unwrap().to_string(), m["op"].as_str().unwrap());
            if let Some((k, i, rev, seq)) = self.mids.borrow().get(&mid).cloned() {
                assert_eq!((k.as_str(), i.as_str()), (kind.as_str(), id.as_str()));
                results.push(json!({ "mutationId": mid, "status": "duplicate", "kind": kind, "id": id, "revision": rev, "seq": seq }));
                continue;
            }
            let base = m["baseRevision"].as_i64().unwrap();
            let cur = self.get(&kind, &id);
            let cur_rev = cur.as_ref().map(|d| d.rev).unwrap_or(0);
            if op == "delete" {
                if cur.as_ref().is_none_or(|d| d.deleted) {
                    results.push(json!({ "mutationId": mid, "status": "missing", "kind": kind, "id": id, "current": self.current(&kind, &id) }));
                    continue;
                }
            }
            if base != cur_rev {
                results.push(json!({ "mutationId": mid, "status": "conflict", "kind": kind, "id": id, "current": self.current(&kind, &id) }));
                continue;
            }
            let rev = cur_rev + 1;
            let deleted = op == "delete";
            let payload = if deleted { None } else { Some(m["payload"].clone()) };
            assert!(deleted || payload.as_ref().unwrap().is_object(), "본문은 객체");
            self.docs.borrow_mut().insert((kind.clone(), id.clone()), Doc { rev, payload, deleted });
            let seq = self.log(&kind, &id, rev, op, Some(&mid));
            self.mids.borrow_mut().insert(mid.clone(), (kind.clone(), id.clone(), rev, seq));
            results.push(json!({ "mutationId": mid, "status": "applied", "kind": kind, "id": id, "revision": rev, "seq": seq, "deleted": deleted }));
        }
        if self.lose_reply.replace(false) {
            return Err(ApiErr::Net("답을 잃음".into()));
        }
        Ok(json!({ "ok": true, "results": results, "head": self.seq() }))
    }

    fn asset_find(&self, sha: &str) -> Result<Option<String>, ApiErr> {
        if self.offline.get() {
            return Err(ApiErr::Net("끊김".into()));
        }
        if self.no_assets.get() {
            return Err(ApiErr::Missing);
        }
        Ok(self.assets.borrow().get(sha).map(|a| a.0.clone()))
    }

    fn asset_put(&self, file: &Path, sha: &str, name: &str, ctype: &str, size: u64) -> Result<String, ApiErr> {
        if self.no_assets.get() {
            return Err(ApiErr::Missing);
        }
        assert!(!name.contains('/') && !name.contains('\\'), "파일 이름만 (경로 없이): {name}");
        let b = std::fs::read(file).unwrap();
        assert_eq!(b.len() as u64, size);
        assert_eq!(hex(&Sha256::digest(&b)), sha, "서버가 다시 계산한 sha256 과 같다");
        self.uploads.set(self.uploads.get() + 1);
        let id = format!("a{}", &sha[..12]);
        self.assets.borrow_mut().entry(sha.to_string()).or_insert((id.clone(), b, ctype.to_string()));
        Ok(id)
    }

    fn asset_get(&self, id: &str) -> Result<(Vec<u8>, String), ApiErr> {
        self.assets.borrow().values().find(|a| a.0 == id).map(|a| (a.1.clone(), a.2.clone())).ok_or(ApiErr::Missing)
    }
}

/// 출력 한 겹(OUT)은 전역이라 이 시험들은 하나씩
static SERIAL: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

fn temp_core(tag: &str) -> (Shared, PathBuf) {
    let dir = std::env::temp_dir().join(format!("wp-cloudsync-{tag}-{}-{}", std::process::id(), short_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bible.db"), b"").unwrap();
    let c = Core::open(dir.clone(), dir.clone(), None).unwrap();
    (Arc::new(crate::lockcheck::Mutex::new(c)), dir)
}

fn cue(id: &str, name: &str, items: Vec<Value>) -> Cue {
    Cue { id: id.into(), name: name.into(), service_date: "2026-11-29".into(), start_time: "11:00".into(), items, memo: String::new(), pinned: false, template: false }
}

fn save_cue(core: &Shared, c: &Cue) {
    let mut g = core.lock();
    g.store.save_cue(c).unwrap();
    g.cues = g.store.list_cues();
}

fn cue_name(core: &Shared, id: &str) -> Option<String> {
    local_one(&core.lock(), "cue", id).map(|v| v["name"].as_str().unwrap().to_string())
}

const URL: &str = "https://wp.example.workers.dev";

#[test]
fn off_by_default_and_needs_cloud_connection() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("off");
    let mut c = core.lock();
    assert!(!sync_conf(&c).on && !wanted(&c), "기본 꺼짐");
    set_wanted(&mut c, true);
    assert!(!sync_conf(&c).on, "클라우드 연결(주소 · 열쇠 · 켜기)이 없으면 켜지지 않는다");
    c.settings.update(|d| d["cloud"] = json!({ "enabled": true, "url": URL, "key": "sample.AbCdEfGhJkMnPqRsTuVwXyZ23", "sync": true }));
    let cf = sync_conf(&c);
    assert!(cf.on && cf.church == "sample");
    c.settings.update(|d| d["cloud"]["enabled"] = json!(false));
    assert!(!sync_conf(&c).on, "클라우드 연결을 끄면 동기화도 꺼진다");
    drop(c);
    // 꺼져 있으면 출력 자료는 1단계와 똑같다
    out_off();
    let v = json!({ "slide": { "image": "/lib/file?p=C%3A%5CUsers%5Chong%5Ca.png" } });
    assert_eq!(for_output(v.clone()), v);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn first_full_upload_then_only_changes_and_round_trip() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("full");
    save_cue(&core, &cue("c1", "주일 예배", vec![json!({ "id": "i1", "type": "title", "title": "예배로 부름" })]));
    save_cue(&core, &cue("c2", "수요 예배", vec![]));
    core.lock().save_song(&json!({ "id": "s1abc", "title": "주 하나님 지으신 모든 세계", "lyrics": "[1절]\n주 하나님", "key": "G" })).unwrap();
    // 쓰는 법이 다른 키(기기마다)는 교회 공통 설정에 들어가지 않는다
    core.lock().settings.update(|d| {
        d["obs"]["password"] = json!("비밀");
        d["remote"]["pin"] = json!("123456");
        d["messageTemplates"] = json!([{ "id": "m1", "name": "호출", "text": "{이름}", "seconds": 30 }]);
        d["overlays"] = json!([{ "id": "ov1", "name": "로고", "on": true }]);
    });
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    let templates = core.lock().settings.get()["templates"].as_array().unwrap().len();
    assert_eq!(fake.live("cue").len(), 2);
    assert_eq!(fake.live("song").len(), 1);
    assert_eq!(fake.live("template").len(), templates);
    let st = fake.get("settings", CHURCH).unwrap().payload.unwrap();
    assert_eq!(st["messageTemplates"][0]["name"], "호출");
    assert!(st["overlays"][0].get("on").is_none(), "오버레이 켜짐은 PC 마다");
    for k in ["obs", "remote", "port", "syncDir", "devices", "cameras", "ui", "uiLang", "hymnFolders", "imageFolders", "cloud", "ai"] {
        assert!(st.get(k).is_none(), "기기 설정 {k} 는 올리지 않는다");
    }
    assert!(!fake.all_text().contains("비밀") && !fake.all_text().contains("123456"));
    let c1 = fake.get("cue", "c1").unwrap().payload.unwrap();
    assert_eq!(c1, json!({ "id": "c1", "name": "주일 예배", "serviceDate": "2026-11-29", "startTime": "11:00", "items": [{ "id": "i1", "type": "title", "title": "예배로 부름" }], "memo": "", "pinned": false, "template": false }), "설치판 Cue 모양 그대로");
    let first_pushes = fake.pushes.get();
    assert!(first_pushes >= 1);
    // 아무것도 안 바뀌면 올리지 않는다 (받기만)
    eng.tick().unwrap();
    assert_eq!(fake.pushes.get(), first_pushes, "바뀐 것이 없으면 올리지 않는다");
    // 하나 고치면 그것만
    let mut c = core.lock().store.get_cue("c1").unwrap();
    c.name = "주일 예배 2부".into();
    save_cue(&core, &c);
    let before = fake.seq();
    eng.tick().unwrap();
    assert_eq!(fake.seq(), before + 1, "바뀐 큐시트 하나만 올린다");
    assert_eq!(fake.get("cue", "c1").unwrap().payload.unwrap()["name"], "주일 예배 2부");
    assert_eq!(fake.get("cue", "c1").unwrap().rev, 2);
    // 클라우드(웹)에서 고친 것이 설치판에 돌아온다
    let mut p = fake.get("cue", "c2").unwrap().payload.unwrap();
    p["name"] = json!("수요 예배 (웹에서 고침)");
    p["items"] = json!([{ "id": "w1", "type": "text", "body": "광고" }]);
    fake.web_put("cue", "c2", p);
    fake.web_put("song", "s9web", json!({ "id": "s9web", "title": "웹에서 넣은 찬양", "lyrics": "가사" }));
    eng.tick().unwrap();
    assert_eq!(cue_name(&core, "c2").as_deref(), Some("수요 예배 (웹에서 고침)"));
    assert_eq!(core.lock().store.get_cue("c2").unwrap().items[0]["body"], "광고");
    assert_eq!(core.lock().get_song("s9web").unwrap()["title"], "웹에서 넣은 찬양");
    assert!(core.lock().cues.iter().any(|c| c.name == "수요 예배 (웹에서 고침)"), "큐시트 목록도 새로");
    // 받은 것을 다시 올리지 않는다 (메아리 없음)
    let before = fake.seq();
    eng.tick().unwrap();
    assert_eq!(fake.seq(), before, "받은 것을 되돌려 올리지 않는다");
    // 기록이 저장되어 다시 켜도 처음부터 올리지 않는다
    let pushes = fake.pushes.get();
    let mut eng2 = Engine::new(&core, &fake, "sample", URL);
    eng2.tick().unwrap();
    assert_eq!(fake.pushes.get(), pushes, "다시 켜도 전체를 다시 올리지 않는다");
    drop(eng);
    drop(eng2);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn conflict_keeps_both_as_copy_never_overwrites() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("conflict");
    save_cue(&core, &cue("c1", "주일", vec![]));
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    // 양쪽에서 같은 큐시트를 고침
    let mut p = fake.get("cue", "c1").unwrap().payload.unwrap();
    p["name"] = json!("주일 (웹)");
    fake.web_put("cue", "c1", p);
    let mut c = core.lock().store.get_cue("c1").unwrap();
    c.name = "주일 (이 PC)".into();
    save_cue(&core, &c);
    eng.tick().unwrap();
    assert_eq!(cue_name(&core, "c1").as_deref(), Some("주일 (웹)"), "원래 자리는 클라우드 판");
    let names: Vec<String> = core.lock().store.list_cues().into_iter().map(|c| c.name).collect();
    let copy = names.iter().find(|n| n.starts_with("주일 (이 PC) (충돌 사본")).cloned().expect("이 PC 판은 충돌 사본으로");
    assert_eq!(eng.report.conflicts, vec![copy.clone()]);
    assert!(fake.live("cue").iter().any(|(_, p)| p["name"] == copy.as_str()), "사본도 클라우드에 올라가 웹에서 고를 수 있다");
    assert_eq!(fake.get("cue", "c1").unwrap().payload.unwrap()["name"], "주일 (웹)", "클라우드 판을 덮어쓰지 않았다");
    // 올리는 사이에 남이 고친 경우 (409) 도 같은 규칙
    let mut c = core.lock().store.get_cue("c1").unwrap();
    c.name = "주일 (이 PC 둘째)".into();
    save_cue(&core, &c);
    let mut p = fake.get("cue", "c1").unwrap().payload.unwrap();
    p["name"] = json!("주일 (웹 둘째)");
    fake.web_put("cue", "c1", p);
    fake.hide_once.set(true);
    eng.tick().unwrap();
    assert_eq!(cue_name(&core, "c1").as_deref(), Some("주일 (웹 둘째)"));
    assert_eq!(fake.get("cue", "c1").unwrap().payload.unwrap()["name"], "주일 (웹 둘째)");
    assert!(core.lock().store.list_cues().iter().any(|c| c.name.starts_with("주일 (이 PC 둘째) (충돌 사본")));
    // 찬양 · 템플릿도 사본
    core.lock().save_song(&json!({ "id": "s1abc", "title": "찬양" })).unwrap();
    eng.tick().unwrap();
    fake.web_put("song", "s1abc", json!({ "id": "s1abc", "title": "찬양 (웹)", "lyrics": "" }));
    core.lock().save_song(&json!({ "id": "s1abc", "title": "찬양 (PC)", "lyrics": "x" })).unwrap();
    eng.tick().unwrap();
    assert_eq!(core.lock().get_song("s1abc").unwrap()["title"], "찬양 (웹)");
    assert!(core.lock().list_songs("").iter().any(|s| s["title"].as_str().unwrap().starts_with("찬양 (PC) (충돌 사본")));
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn remote_delete_is_recoverable_and_edits_are_not_deleted() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("delete");
    save_cue(&core, &cue("c1", "지울 큐시트", vec![]));
    save_cue(&core, &cue("c2", "고치던 큐시트", vec![]));
    core.lock().save_song(&json!({ "id": "s1abc", "title": "지울 찬양", "lyrics": "가사 원본" })).unwrap();
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    fake.web_delete("cue", "c1");
    fake.web_delete("song", "s1abc");
    fake.web_delete("cue", "c2");
    let mut c = core.lock().store.get_cue("c2").unwrap();
    c.memo = "이 PC 에서 고침".into();
    save_cue(&core, &c);
    eng.tick().unwrap();
    assert!(cue_name(&core, "c1").is_none(), "목록에서는 빠지고");
    assert!(crate::cues::list_trash(&core.lock().store).iter().any(|x| x["id"] == "c1"), "휴지통에 남는다 (되살릴 수 있음)");
    assert!(core.lock().get_song("s1abc").is_none());
    let removed: Vec<_> = std::fs::read_dir(dir.join("cloudsync").join("removed")).unwrap().filter_map(|e| e.ok()).collect();
    assert!(removed.iter().any(|e| std::fs::read_to_string(e.path()).unwrap().contains("가사 원본")), "지운 찬양은 사본 파일로");
    assert_eq!(cue_name(&core, "c2").as_deref(), Some("고치던 큐시트"), "고치던 것은 지우지 않는다");
    assert_eq!(eng.report.kept, vec!["고치던 큐시트".to_string()]);
    eng.tick().unwrap();
    let d = fake.get("cue", "c2").unwrap();
    assert!(!d.deleted && d.payload.unwrap()["memo"] == "이 PC 에서 고침", "클라우드에서도 되살아난다");
    // 이 PC 에서 지운 것 → 클라우드도 지움 (묘비, 서버가 되살릴 수 있음)
    crate::cues::trash(&core.lock().store, "c2").unwrap();
    eng.tick().unwrap();
    assert!(fake.get("cue", "c2").unwrap().deleted);
    // 쓰는 중인 템플릿은 클라우드에서 지워도 남긴다
    let active = core.lock().settings.get()["activeTemplates"]["main"].as_str().unwrap().to_string();
    fake.web_delete("template", &active);
    eng.tick().unwrap();
    assert!(local_one(&core.lock(), "template", &active).is_some());
    eng.tick().unwrap();
    assert!(!fake.get("template", &active).unwrap().deleted, "쓰는 중인 템플릿은 되살림");
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lost_reply_is_resent_with_same_mutation_id() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("outbox");
    save_cue(&core, &cue("c1", "주일", vec![]));
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    fake.lose_reply.set(true);
    assert!(eng.tick().is_err(), "답을 잃음");
    assert!(!eng.st.outbox.is_empty(), "보낸 것을 저장해 두었다");
    let saved = State::load(&dir.join("cloudsync"), "sample", URL);
    assert_eq!(saved.outbox.len(), eng.st.outbox.len(), "디스크에도 (꺼져도 남음)");
    let applied = fake.seq();
    eng.tick().unwrap();
    assert!(eng.st.outbox.is_empty());
    assert_eq!(fake.get("cue", "c1").unwrap().rev, 1, "두 번 들어가지 않았다");
    assert_eq!(fake.seq(), applied, "같은 mutationId → duplicate");
    // 인터넷이 끊겨도 설치판 자료는 그대로, 다시 이으면 이어서
    fake.offline.set(true);
    let mut c = core.lock().store.get_cue("c1").unwrap();
    c.name = "끊긴 동안 고침".into();
    save_cue(&core, &c);
    assert!(matches!(eng.tick(), Err(ApiErr::Net(_))));
    assert_eq!(cue_name(&core, "c1").as_deref(), Some("끊긴 동안 고침"));
    fake.offline.set(false);
    eng.tick().unwrap();
    assert_eq!(fake.get("cue", "c1").unwrap().payload.unwrap()["name"], "끊긴 동안 고침");
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

/// PC 경로 · 사용자 이름이 클라우드로 새지 않는다
fn leak_free(text: &str, user_dir: &str) {
    assert!(!text.contains(":\\\\") && !text.contains(":/Users") && !text.contains("%3A%5C"), "드라이브 경로가 남음: {text}");
    assert!(!text.contains("/lib/file?p="), "PC 경로 주소가 남음");
    assert!(!text.contains(user_dir), "사용자 폴더 이름이 남음");
    assert!(!text.contains("file:///"));
}

#[test]
fn file_paths_become_assets_and_never_leak() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("assets");
    // 사용자 폴더처럼 보이는 곳의 그림
    let user = format!("홍길동-{}", short_id("u"));
    let pics = dir.join("Users").join(&user).join("Pictures");
    std::fs::create_dir_all(&pics).unwrap();
    let img = pics.join("배경 (1).png");
    std::fs::write(&img, b"\x89PNG\r\n\x1a\nfake").unwrap();
    let vid = pics.join("intro.MP4");
    std::fs::write(&vid, b"video-bytes").unwrap();
    let media = core.lock().media_dir.join("logo.png");
    std::fs::write(&media, b"logo-bytes").unwrap();
    let url = crate::library::file_url(&img);
    save_cue(&core, &cue("c1", "그림 예배", vec![
        json!({ "id": "a", "type": "image", "image": url }),
        json!({ "id": "b", "type": "image", "image": img.to_string_lossy() }),
        json!({ "id": "c", "type": "video", "image": crate::library::file_url(&vid) }),
        json!({ "id": "d", "type": "image", "image": "/media/logo.png" }),
        json!({ "id": "e", "type": "image", "image": format!("file:///{}", img.to_string_lossy().replace('\\', "/")) }),
        json!({ "id": "f", "type": "text", "body": "그대로 둘 글", "bg": format!("url(\"{url}\") center") }),
        json!({ "id": "g", "type": "image", "image": dir.join("없는 파일.png").to_string_lossy() }),
        json!({ "id": "p", "type": "hymn", "file": pics.join("광고.pptx").to_string_lossy() }),
    ]));
    std::fs::write(pics.join("광고.pptx"), b"PK-pptx").unwrap();
    core.lock().settings.update(|d| d["templates"][0]["background"]["image"] = json!(url.clone()));
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    let text = fake.all_text();
    leak_free(&text, &user);
    assert_eq!(fake.uploads.get(), 3, "같은 파일(sha256)은 한 번만 올린다 — 그림 · 영상 · 로고");
    let items = fake.get("cue", "c1").unwrap().payload.unwrap()["items"].clone();
    let a = items[0]["image"].as_str().unwrap();
    assert!(a.starts_with("/api/v1/assets/") && a.ends_with("?e=.png"), "{a}");
    assert_eq!(items[1]["image"], a, "같은 파일은 같은 자산");
    assert_eq!(items[4]["image"], a);
    assert!(items[2]["image"].as_str().unwrap().ends_with(".mp4"), "영상은 주소 끝에 확장자 (render.js isVideo)");
    assert!(items[3]["image"].as_str().unwrap().starts_with("/api/v1/assets/"), "/media/ 도 올림 (웹 화면이 설치판 없이 보이게)");
    assert!(items[5]["bg"].as_str().unwrap().starts_with("url(\"/api/v1/assets/"), "글 안에 낀 주소도");
    assert!(items[6]["image"].as_str().unwrap().starts_with("wp-local:"), "없는 파일은 가림표");
    assert!(items[7]["file"].as_str().unwrap().starts_with("wp-local:"), "저장소가 받지 않는 종류(pptx)는 올리지 않고 가림표");
    assert!(fake.get("template", fake.live("template")[0].0.as_str()).is_some());
    // 이 PC 는 그대로 (원래 주소)
    assert_eq!(core.lock().store.get_cue("c1").unwrap().items[0]["image"], url.as_str());
    // 웹에서 고친 문서가 돌아오면 원래 주소로 되돌리고, 모르는 자산은 받아서 media 폴더에
    let mut p = fake.get("cue", "c1").unwrap().payload.unwrap();
    p["name"] = json!("그림 예배 (웹)");
    let web_png = b"\x89PNG\r\n\x1a\nweb".to_vec();
    let sha = hex(&Sha256::digest(&web_png));
    fake.assets.borrow_mut().insert(sha.clone(), ("aweb1".into(), web_png.clone(), "image/png".into()));
    p["items"].as_array_mut().unwrap().push(json!({ "id": "h", "type": "image", "image": "/api/v1/assets/aweb1" }));
    fake.web_put("cue", "c1", p);
    eng.tick().unwrap();
    let back = core.lock().store.get_cue("c1").unwrap();
    assert_eq!(back.name, "그림 예배 (웹)");
    assert_eq!(back.items[0]["image"], url.as_str(), "자산 주소 → 이 PC 의 원래 주소");
    assert_eq!(back.items[6]["image"], dir.join("없는 파일.png").to_string_lossy().as_ref(), "가림표 → 원래 주소");
    assert_eq!(back.items[7]["file"], pics.join("광고.pptx").to_string_lossy().as_ref(), "가림표 pptx → 원래 경로");
    let h = back.items[8]["image"].as_str().unwrap();
    assert!(h.starts_with("/media/cloud-") && h.ends_with(".png"), "{h}");
    assert_eq!(std::fs::read(core.lock().media_dir.join(&h[7..])).unwrap(), web_png);
    // 되돌린 것을 다시 올려도 같은 내용 (메아리 없음)
    let before = fake.seq();
    eng.tick().unwrap();
    assert_eq!(fake.seq(), before);
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn without_asset_store_paths_are_masked_then_filled_later() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("noassets");
    let user = format!("김사관-{}", short_id("u"));
    let pics = dir.join("Users").join(&user);
    std::fs::create_dir_all(&pics).unwrap();
    let img = pics.join("a.jpg");
    std::fs::write(&img, b"jpeg").unwrap();
    save_cue(&core, &cue("c1", "그림", vec![json!({ "id": "a", "type": "image", "image": crate::library::file_url(&img) })]));
    let fake = Fake::default();
    fake.no_assets.set(true);
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    leak_free(&fake.all_text(), &user);
    assert!(fake.get("cue", "c1").unwrap().payload.unwrap()["items"][0]["image"].as_str().unwrap().starts_with("wp-local:"));
    assert_eq!(eng.assets_off.as_deref(), Some("missing"));
    // 그림·영상 길이 생기면 (30분 뒤 다시 봄 · 가림표 문서는 10분 뒤 다시 올림)
    fake.no_assets.set(false);
    eng.assets_off = None;
    eng.assets_off_at = None;
    for e in eng.st.docs.values_mut() {
        if e.partial > 0 {
            e.partial = 1;
        }
    }
    eng.tick().unwrap();
    let a = fake.get("cue", "c1").unwrap().payload.unwrap()["items"][0]["image"].as_str().unwrap().to_string();
    assert!(a.starts_with("/api/v1/assets/"), "{a}");
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn output_layer_uses_assets_or_relay_token_without_path() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("output");
    let user = format!("사용자-{}", short_id("u"));
    let pics = dir.join("Users").join(&user);
    std::fs::create_dir_all(&pics).unwrap();
    let img = pics.join("슬라이드.png");
    std::fs::write(&img, b"png!").unwrap();
    let url = crate::library::file_url(&img);
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.publish_out();
    let render = json!({ "slide": { "image": url, "bg": "/shared/assets/x.png", "logo": "/media/none.png" }, "audio": { "src": crate::library::file_url(&pics.join("없음.mp3")) } });
    let out = for_output(render.clone());
    let s = out["slide"]["image"].as_str().unwrap();
    assert!(s.starts_with("/lib/file?t=") && s.ends_with(".png"), "아직 안 올린 것은 경로 대신 중계 표: {s}");
    leak_free(&out.to_string(), &user);
    assert_eq!(out["slide"]["bg"], "/shared/assets/x.png", "프로그램 안 그림은 그대로");
    assert_eq!(out["slide"]["logo"], "/media/none.png", "자료 폴더 이름뿐인 주소는 1단계 중계 그대로");
    // 중계 요청이 오면 표 → 이 PC 경로 (cloud.rs)
    let tok = &s["/lib/file?t=".len()..];
    let q = relay_query(&json!({ "t": tok }));
    assert_eq!(q["p"], img.to_string_lossy().as_ref());
    assert_eq!(relay_query(&json!({ "t": "모르는표.png" }))["p"], "", "모르는 표는 아무 파일도 아님");
    assert_eq!(relay_request(&json!({ "path": "/lib/file", "query": { "p": "x" } }))["query"]["p"], "x", "1단계 주소는 그대로");
    // 영상 열쇠('주소#시각')는 가리고, 출력 화면이 돌려보내면 되돌린다
    let vkey = format!("{url}#1795828365000");
    let out = for_output(json!({ "video": { "key": vkey, "playing": true } }));
    let vk = out["video"]["key"].as_str().unwrap().to_string();
    assert!(vk.starts_with("vk-") && !out.to_string().contains(&user));
    let back = relay_command(&json!({ "t": "cmd", "cmd": "videoMeta", "args": { "key": vk, "duration": 12.5 } }));
    assert_eq!(back["args"]["key"], vkey.as_str(), "설치판은 원래 열쇠로 영상 길이를 안다");
    assert_eq!(relay_command(&json!({ "args": { "key": "v1" } }))["args"]["key"], "v1");
    // 동기화 스레드가 올리면 → 자산 주소
    assert!(eng.serve_output_wants());
    let out = for_output(render.clone());
    let s = out["slide"]["image"].as_str().unwrap();
    assert!(s.starts_with("/api/v1/assets/") && s.contains("?fallback=%2Flib%2Ffile%3Ft%3D") && s.ends_with(".png"), "올린 것은 자산 주소 + 중계 주소(fallback, 경로 대신 표): {s}");
    leak_free(&out.to_string(), &user);
    out_off();
    assert_eq!(for_output(render.clone()), render, "끄면 그대로");
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn church_change_starts_a_fresh_record() {
    let _serial = SERIAL.lock();
    let (core, dir) = temp_core("church");
    save_cue(&core, &cue("c1", "주일", vec![]));
    let fake = Fake::default();
    let mut eng = Engine::new(&core, &fake, "sample", URL);
    eng.tick().unwrap();
    eng.tick().unwrap();
    assert!(eng.st.cursor > 0 && !eng.st.docs.is_empty());
    drop(eng);
    let other = Fake::default();
    let mut eng = Engine::new(&core, &other, "other", URL);
    assert_eq!(eng.st.cursor, 0, "다른 교회는 처음부터");
    eng.tick().unwrap();
    assert_eq!(other.live("cue").len(), 1, "다른 교회에도 전체를 올린다 (섞이지 않음)");
    drop(eng);
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn second_pc_with_same_data_adopts_without_copies() {
    let _serial = SERIAL.lock();
    // 1.x 자료를 복사한 두 PC (같은 id): 둘째 PC 가 붙어도 같은 내용이면 사본이 생기지 않는다
    let (a, da) = temp_core("pc1");
    let (b, db) = temp_core("pc2");
    for core in [&a, &b] {
        save_cue(core, &cue("c1", "주일", vec![json!({ "id": "x", "type": "title", "title": "환영" })]));
        core.lock().save_song(&json!({ "id": "s1abc", "title": "찬양", "lyrics": "가사" })).unwrap();
    }
    let fake = Fake::default();
    Engine::new(&a, &fake, "sample", URL).tick().unwrap();
    let mut eb = Engine::new(&b, &fake, "sample", URL);
    eb.tick().unwrap();
    assert!(eb.report.conflicts.is_empty(), "{:?}", eb.report.conflicts);
    assert_eq!(b.lock().store.list_cues().len(), 1);
    assert_eq!(fake.live("cue").len(), 1);
    drop(eb);
    drop(a);
    drop(b);
    let _ = std::fs::remove_dir_all(da);
    let _ = std::fs::remove_dir_all(db);
}

#[test]
fn path_helpers() {
    let _serial = SERIAL.lock();
    let m = Path::new("D:\\data\\media");
    assert_eq!(local_file(m, "/lib/file?p=C%3A%5CUsers%5Ca%5Cb.png"), Some(PathBuf::from("C:\\Users\\a\\b.png")));
    assert_eq!(local_file(m, "/lib/file?p=50%할인.png"), Some(PathBuf::from("50%할인.png")), "깨진 % 도 터지지 않는다");
    assert_eq!(local_file(m, "/media/a.png"), Some(m.join("a.png")));
    assert_eq!(local_file(m, "/media/../x"), None);
    assert_eq!(local_file(m, "file:///C:/a/b.mp4"), Some(PathBuf::from("C:/a/b.mp4")));
    assert_eq!(local_file(m, "file:///Users/me/a.png"), Some(PathBuf::from("/Users/me/a.png")));
    assert!(local_file(m, "C:\\x.png").is_some() && local_file(m, "\\\\server\\share\\a.png").is_some());
    for s in ["/shared/assets/a.png", "/hymn/k/1.svg", "https://x/a.png", "그냥 글", "C:", "/api/v1/assets/a1"] {
        assert!(local_file(m, s).is_none(), "{s}");
    }
    assert_eq!(parse_asset("/api/v1/assets/abc_1?e=.mp4"), Some(("abc_1".into(), ".mp4".into())));
    assert_eq!(parse_asset("/api/v1/assets/abc"), Some(("abc".into(), String::new())));
    assert_eq!(parse_asset("/api/v1/assets/../x"), None);
    assert!(valid_doc_id("tpl-abc") && valid_doc_id("c1") && !valid_doc_id("-x") && !valid_doc_id("a/b"));
    assert_eq!(digest(&json!({ "b": 1, "a": 2 })), digest(&json!({ "a": 2, "b": 1 })), "칸 차례와 상관없는 지문");
}

// ---------- 끝에서 끝 (로컬 wrangler dev + 로컬 D1) ----------
// cloud/test/sync-e2e/run.mjs 가 로컬 클라우드를 띄우고 이 시험을 부른다. 혼자 돌리지 않는다 (#[ignore]).
//   WP_SYNC_E2E_URL  로컬 클라우드 (그림·영상 길은 가짜 중계가 받음)  WP_SYNC_E2E_KEY  설치판 연결 열쇠
//   WP_SYNC_E2E_DATA 설치판 자료 복사본 폴더 (bible.db · settings.json)
struct Web {
    url: String,
    auth: String,
    agent: ureq::Agent,
}

impl Web {
    fn call(&self, method: &str, path: &str, if_match: Option<i64>, body: Option<&Value>) -> (u16, Value) {
        let u = format!("{}{path}", self.url);
        let mut b = ureq::http::Request::builder().method(method).uri(&u).header("authorization", &self.auth);
        if let Some(r) = if_match {
            b = b.header("if-match", r.to_string());
        }
        let resp = match body {
            Some(v) => self.agent.run(b.header("content-type", "application/json").body(v.to_string()).unwrap()),
            None => self.agent.run(b.body(()).unwrap()),
        }
        .unwrap();
        let code = resp.status().as_u16();
        let mut resp = resp;
        (code, resp.body_mut().with_config().limit(64 << 20).read_json().unwrap_or(Value::Null))
    }
    fn doc(&self, kind: &str, id: &str) -> Value {
        let (c, v) = self.call("GET", &format!("/api/v1/documents/{kind}/{id}"), None, None);
        assert_eq!(c, 200, "{kind}/{id}: {v}");
        v
    }
    /// 문서 전체 (본문까지)
    fn all(&self, kind: &str) -> Vec<Value> {
        let mut out = vec![];
        let mut cursor = String::new();
        loop {
            let q = if cursor.is_empty() { String::new() } else { format!("&cursor={cursor}") };
            let (c, v) = self.call("GET", &format!("/api/v1/documents?kind={kind}&include=payload&limit=200{q}"), None, None);
            assert_eq!(c, 200, "{v}");
            out.extend(v["documents"].as_array().cloned().unwrap_or_default());
            match v["next"].as_str() {
                Some(n) => cursor = n.to_string(),
                None => break,
            }
        }
        out
    }
}

#[test]
#[ignore]
fn e2e_local_cloud() {
    let (Ok(url), Ok(key), Ok(data)) = (std::env::var("WP_SYNC_E2E_URL"), std::env::var("WP_SYNC_E2E_KEY"), std::env::var("WP_SYNC_E2E_DATA")) else {
        panic!("cloud/test/sync-e2e/run.mjs 로 돌린다");
    };
    let church = key.split_once('.').unwrap().0.to_string();
    let data = PathBuf::from(data);
    let core: Shared = Arc::new(crate::lockcheck::Mutex::new(Core::open(data.clone(), data.clone(), None).unwrap()));
    {
        let mut c = core.lock();
        let (u, k) = (url.clone(), key.clone());
        c.settings.update(|d| d["cloud"] = json!({ "enabled": true, "url": u, "key": k, "sync": true }));
        assert!(sync_conf(&c).on);
    }
    let web = Web { url: url.clone(), auth: format!("Bearer {key}"), agent: ureq::Agent::config_builder().http_status_as_error(false).build().into() };
    let api = Http::new(&url, &key);
    let mut eng = Engine::new(&core, &api, &church, &url);
    let (n_cue, n_song, n_tpl) = {
        let c = core.lock();
        let d = local_docs(&c);
        let n = |k: &str| d.keys().filter(|x| x.starts_with(&format!("{k}/"))).count();
        (n("cue"), n("song"), n("template"))
    };
    println!("설치판 복사본: 큐시트 {n_cue} · 찬양 {n_song} · 템플릿 {n_tpl}");
    // 1) 처음 한 번 전체 올리기
    let t0 = Instant::now();
    eng.tick().unwrap();
    println!("처음 올리기 {:?}: 올림 {} · 파일 {} · 문제 {:?}", t0.elapsed(), eng.report.pushed, eng.report.uploaded, eng.report.errors);
    let cues = web.all("cue");
    assert_eq!(cues.len(), n_cue, "큐시트가 다 올라갔다");
    assert_eq!(web.all("song").len(), n_song, "찬양이 다 올라갔다");
    assert_eq!(web.all("template").len(), n_tpl);
    // PC 경로 · 사용자 이름이 클라우드에 없다
    let mut text = String::new();
    for k in KINDS {
        for d in web.all(k) {
            text.push_str(&d["payload"].to_string());
        }
    }
    let user = std::env::var("USERNAME").unwrap_or_default();
    leak_free(&text, if user.len() >= 3 { &user } else { "\u{0}없음" });
    println!("클라우드 문서 본문 {} 바이트, 자산 주소 {}개, 가림표 {}개", text.len(), text.matches("/api/v1/assets/").count(), text.matches("wp-local:").count());
    // 바뀐 것이 없으면 아무것도 올리지 않는다
    eng.tick().unwrap();
    eng.tick().unwrap();
    assert_eq!(eng.report.pushed, 0, "메아리 없음");
    // 2) 설치판에서 큐시트 하나를 고치면 → D1
    let id = cues[0]["id"].as_str().unwrap().to_string();
    let mut c = core.lock().store.get_cue(&id).unwrap();
    c.name = format!("{} · PC 에서 고침", c.name);
    let pc_name = c.name.clone();
    save_cue(&core, &c);
    eng.tick().unwrap();
    let d = web.doc("cue", &id);
    assert_eq!(d["payload"]["name"], pc_name.as_str(), "설치판 수정이 D1 에");
    let rev = d["revision"].as_i64().unwrap();
    println!("PC → D1: {id} 판 {rev}");
    // 3) D1 쪽 수정 → 설치판
    let mut p = d["payload"].clone();
    p["name"] = json!("웹에서 고친 큐시트");
    p["memo"] = json!("웹 편집");
    let (code, r) = web.call("PUT", &format!("/api/v1/documents/cue/{id}"), Some(rev), Some(&p));
    assert_eq!(code, 200, "{r}");
    eng.tick().unwrap();
    assert_eq!(cue_name(&core, &id).as_deref(), Some("웹에서 고친 큐시트"), "D1 수정이 설치판에");
    assert_eq!(core.lock().store.get_cue(&id).unwrap().memo, "웹 편집");
    println!("D1 → PC: 이름 · 메모 돌아옴");
    // 4) 충돌: 양쪽에서 고침 → 클라우드 판이 원래 자리, PC 판은 충돌 사본 (양쪽에 다 있음)
    let d = web.doc("cue", &id);
    let mut p = d["payload"].clone();
    p["name"] = json!("충돌 시험 · 웹 판");
    let (code, _) = web.call("PUT", &format!("/api/v1/documents/cue/{id}"), d["revision"].as_i64(), Some(&p));
    assert_eq!(code, 200);
    let mut c = core.lock().store.get_cue(&id).unwrap();
    c.name = "충돌 시험 · PC 판".into();
    save_cue(&core, &c);
    eng.tick().unwrap();
    assert_eq!(cue_name(&core, &id).as_deref(), Some("충돌 시험 · 웹 판"));
    assert_eq!(eng.report.conflicts.len(), 1, "{:?}", eng.report);
    let copy = eng.report.conflicts[0].clone();
    assert!(copy.starts_with("충돌 시험 · PC 판 (충돌 사본"));
    assert!(core.lock().store.list_cues().iter().any(|x| x.name == copy));
    eng.tick().unwrap();
    assert!(web.all("cue").iter().any(|d| d["payload"]["name"] == copy.as_str()), "사본이 D1 에도");
    assert_eq!(web.doc("cue", &id)["payload"]["name"], "충돌 시험 · 웹 판", "D1 판을 덮어쓰지 않았다");
    println!("충돌: 원래 자리 = 웹 판, 사본 = {copy} (양쪽)");
    // 5) 출력 자료에 PC 경로가 없다
    let out = {
        let c = core.lock();
        for_output(c.output_payload("main")).to_string() + &for_output(c.stage_payload()).to_string()
    };
    leak_free(&out, if user.len() >= 3 { &user } else { "\u{0}없음" });
    // 6) 올린 파일을 자산 길로 다시 읽을 수 있다
    if let Some((_, f)) = eng.st.files.iter().find(|(_, f)| !f.asset.is_empty()) {
        let (bytes, _) = api.asset_get(&f.asset).unwrap();
        assert_eq!(hex(&Sha256::digest(&bytes)), f.sha, "올린 파일 = 받은 파일");
        println!("자산 {} 다시 받음 ({} 바이트)", f.asset, bytes.len());
    }
    println!("끝에서 끝 통과");
}
