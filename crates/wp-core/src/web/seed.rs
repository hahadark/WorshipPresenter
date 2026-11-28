//! [웹판] 견본 자료 넣기: 브라우저 안 DB 가 비어 있을 때 `web/app/sample/seed.json` 을 넣는다.
//! 클라우드 저장 층(docs/web/architecture.md)이 붙기 전까지 성경 검색 · 찬양 · 큐시트를 시험하려는 것.
//! 성경은 공공 영역(개역한글) 몇 장, 찬양 가사는 시험용으로 직접 쓴 글만 (실제 교회 자료는 넣지 않는다).
use crate::core::Core;
use crate::db::Cue;
use crate::importer::{replace_version, Data, Row};
use serde_json::Value;

fn rows(v: &Value) -> Vec<Row> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some(Row { book: r[0].as_i64()?, chapter: r[1].as_i64()?, verse: r[2].as_i64()?, text: r[3].as_str()?.to_string() }))
        .collect()
}

/// 넣은 것: (번역본, 찬양, 큐시트) 수. 이미 번역본이 있으면 아무것도 하지 않는다
pub fn apply(core: &mut Core, seed: &Value) -> anyhow::Result<(usize, usize, usize)> {
    if !core.store.list_versions().is_empty() {
        return Ok((0, 0, 0));
    }
    let (mut nv, mut ns, mut nc) = (0, 0, 0);
    for v in seed["versions"].as_array().into_iter().flatten() {
        let d = Data { verses: rows(&v["verses"]), headings: rows(&v["headings"]), notes: rows(&v["notes"]) };
        replace_version(&core.store.db, &v["meta"], &d)?;
        nv += 1;
    }
    core.store.reload_structure()?;
    crate::songs::ensure_schema(&core.store.db)?;
    for s in seed["songs"].as_array().into_iter().flatten() {
        crate::songs::save_song(&core.store.db, s)?;
        ns += 1;
    }
    let mut first = None;
    for c in seed["cues"].as_array().into_iter().flatten() {
        let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
        let cue = Cue { id: s("id"), name: s("name"), service_date: s("service_date"), start_time: s("start_time"), items: c["items"].as_array().cloned().unwrap_or_default(), memo: String::new(), pinned: false, template: false };
        core.store.save_cue(&cue)?;
        first.get_or_insert(cue.id.clone());
        nc += 1;
    }
    core.cues = core.store.list_cues();
    if let Some(v) = seed["versions"][0]["meta"]["id"].as_str() {
        core.set_version(v);
    }
    if let Some(id) = first {
        core.open_cue(&id);
    }
    Ok((nv, ns, nc))
}

// ---------------- 문서 꺼내기 · 넣기 (웹판 저장 층) ----------------
// 모양은 계약 docs/web/architecture.md 6절의 documents(kind, id, payload) — payload 는 설치판 JSON 그대로.
// 지금은 브라우저 안(localStorage)에, 2단계에서는 같은 문서를 /api/v1/documents 로 (wp-app web::store)

pub fn cue_doc(c: &Cue) -> Value {
    serde_json::json!({ "id": c.id, "name": c.name, "serviceDate": c.service_date, "startTime": c.start_time, "items": c.items, "memo": c.memo, "pinned": c.pinned, "template": c.template })
}

/// 큐시트 · 찬양 전부: { cue: [payload…], song: [payload…], active } (열린 큐시트는 화면에 있는 것이 가장 새것)
pub fn export_docs(core: &Core) -> Value {
    let mut cues: Vec<Value> = core.store.list_cues().iter().filter_map(|s| core.store.get_cue(&s.id)).map(|c| cue_doc(&c)).collect();
    if let Some(open) = &core.cue {
        let d = cue_doc(open);
        match cues.iter_mut().find(|x| x["id"] == d["id"]) {
            Some(x) => *x = d,
            None => cues.push(d),
        }
    }
    let songs: Vec<Value> = crate::songs::list_songs(&core.store.db, "").iter().filter_map(|s| crate::songs::get_song(&core.store.db, s["id"].as_str()?)).collect();
    serde_json::json!({ "cue": cues, "song": songs, "active": core.cue.as_ref().map(|c| c.id.clone()) })
}

/// export_docs 로 꺼낸 것을 DB 에 넣는다 (같은 id 는 바꿈) → 넣은 (큐시트, 찬양) 수
pub fn import_docs(core: &mut Core, docs: &Value) -> anyhow::Result<(usize, usize)> {
    crate::songs::ensure_schema(&core.store.db)?;
    let (mut nc, mut ns) = (0, 0);
    for c in docs["cue"].as_array().into_iter().flatten() {
        let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
        if s("id").is_empty() {
            continue;
        }
        let cue = Cue { id: s("id"), name: s("name"), service_date: s("serviceDate"), start_time: s("startTime"), items: c["items"].as_array().cloned().unwrap_or_default(), memo: s("memo"), pinned: c["pinned"].as_bool().unwrap_or(false), template: c["template"].as_bool().unwrap_or(false) };
        core.store.save_cue(&cue)?;
        nc += 1;
    }
    for s in docs["song"].as_array().into_iter().flatten() {
        crate::songs::save_song(&core.store.db, s)?;
        ns += 1;
    }
    core.cues = core.store.list_cues();
    if let Some(id) = docs["active"].as_str() {
        core.open_cue(id);
    }
    Ok((nc, ns))
}
