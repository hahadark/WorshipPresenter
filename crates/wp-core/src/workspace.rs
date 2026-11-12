//! Electron 배치를 보존하는 Rust 전용 작업공간 v2.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pane {
    Cue,
    #[serde(alias = "Bible")]
    Main,
    Quick,
    Library,
    Stage,
    Media,
    Preview,
    Live,
}
impl Pane {
    pub const ALL: [Self; 8] = [
        Self::Cue,
        Self::Main,
        Self::Quick,
        Self::Library,
        Self::Stage,
        Self::Media,
        Self::Preview,
        Self::Live,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Cue => "큐시트 · 성경 책",
            Self::Main => "본문 · 편집",
            Self::Quick => "빠른 설정",
            Self::Library => "라이브러리",
            Self::Stage => "무대·타이머",
            Self::Media => "미디어",
            Self::Preview => "미리보기",
            Self::Live => "LIVE",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Pane(Pane),
    Tabs {
        panes: Vec<Pane>,
        active: Pane,
        #[serde(default)]
        collapsed: bool,
    },
    Split {
        horizontal: bool,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}
impl Node {
    pub fn split(horizontal: bool, ratio: f32, a: Self, b: Self) -> Self {
        Self::Split {
            horizontal,
            ratio,
            a: Box::new(a),
            b: Box::new(b),
        }
    }
    pub fn tabs(panes: Vec<Pane>) -> Self {
        Self::Tabs {
            active: panes[0],
            panes,
            collapsed: false,
        }
    }
    pub fn contains(&self, p: Pane) -> bool {
        match self {
            Self::Pane(x) => *x == p,
            Self::Tabs { panes, .. } => panes.contains(&p),
            Self::Split { a, b, .. } => a.contains(p) || b.contains(p),
        }
    }
    pub fn count(&self) -> usize {
        match self {
            Self::Pane(_) => 1,
            Self::Tabs { panes, .. } => panes.len(),
            Self::Split { a, b, .. } => a.count() + b.count(),
        }
    }
    pub fn remove(self, p: Pane) -> Option<Self> {
        match self {
            Self::Pane(x) => {
                if x == p {
                    None
                } else {
                    Some(Self::Pane(x))
                }
            }
            Self::Tabs {
                mut panes,
                mut active,
                collapsed,
            } => {
                panes.retain(|x| *x != p);
                if panes.is_empty() {
                    None
                } else {
                    if active == p {
                        active = panes[0]
                    }
                    Some(Self::Tabs {
                        panes,
                        active,
                        collapsed,
                    })
                }
            }
            Self::Split {
                horizontal,
                ratio,
                a,
                b,
            } => match (a.remove(p), b.remove(p)) {
                (Some(a), Some(b)) => Some(Self::split(horizontal, ratio, a, b)),
                (a, b) => a.or(b),
            },
        }
    }
    pub fn insert(&mut self, p: Pane, target: Pane, side: u8) {
        match self {
            Self::Split { a, b, .. } => {
                if a.contains(target) {
                    a.insert(p, target, side)
                } else {
                    b.insert(p, target, side)
                }
            }
            _ if self.contains(target) => {
                if side == 4 {
                    match self {
                        Self::Tabs { panes, active, .. } => {
                            panes.push(p);
                            *active = p
                        }
                        Self::Pane(t) => *self = Self::tabs(vec![*t, p]),
                        _ => {}
                    }
                } else {
                    let old = self.clone();
                    *self = if side == 0 || side == 2 {
                        Self::split(side < 2, 0.5, Self::Pane(p), old)
                    } else {
                        Self::split(side < 2, 0.5, old, Self::Pane(p))
                    }
                }
            }
            _ => {}
        }
    }
    pub fn group(&self, p: Pane) -> Option<Node> {
        match self {
            Self::Split { a, b, .. } => a.group(p).or_else(|| b.group(p)),
            n if n.contains(p) => Some(n.clone()),
            _ => None,
        }
    }
    pub fn rendered(&self, p: Pane) -> bool {
        match self {
            Self::Pane(x) => *x == p,
            Self::Tabs { active, collapsed, .. } => *active == p && !collapsed,
            Self::Split { a, b, .. } => a.rendered(p) || b.rendered(p),
        }
    }
    pub fn folded(&self) -> bool {
        matches!(self, Self::Tabs { collapsed: true, .. })
    }
    /// 접힌 경계는 저장 비율을 바꾸지 않는다. 펼치면 접기 전 비율을 쓴다.
    pub fn resize_ratio(ratio: &mut f32, a: &Node, b: &Node, value: f32) {
        if !a.folded() && !b.folded() && value.is_finite() {
            *ratio = value.clamp(0.08, 0.92);
        }
    }
    fn valid(&self, seen: &mut Vec<Pane>, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        match self {
            Self::Pane(p) => {
                if seen.contains(p) {
                    false
                } else {
                    seen.push(*p);
                    true
                }
            }
            Self::Tabs { panes, active, .. } => {
                !panes.is_empty()
                    && panes.contains(active)
                    && panes.iter().all(|p| Self::Pane(*p).valid(seen, depth + 1))
            }
            Self::Split { ratio, a, b, .. } => {
                ratio.is_finite()
                    && (0.08..=0.92).contains(ratio)
                    && a.valid(seen, depth + 1)
                    && b.valid(seen, depth + 1)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    pub pane: Pane,
    #[serde(default)]
    pub group: Option<Node>,
    #[serde(default)]
    pub position: Option<[f32; 2]>,
    #[serde(default)]
    pub size: Option<[f32; 2]>,
}
impl Floating {
    pub fn node(&self) -> Node { self.group.clone().unwrap_or(Node::Pane(self.pane)) }
    fn contains(&self, p: Pane) -> bool { self.node().contains(p) }
    fn valid(&self, seen: &mut Vec<Pane>) -> bool {
        let node = self.node();
        !matches!(node, Node::Split { .. }) && node.contains(self.pane)
            && node.valid(seen, 0)
            && self.position.is_none_or(|v| v.iter().all(|x| x.is_finite()))
            && self.size.is_none_or(|v| v.iter().all(|x| x.is_finite() && *x >= 100.0 && *x <= 20000.0))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub name: String,
    pub layout: Option<Node>,
    #[serde(default)]
    pub floating: Vec<Floating>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u8,
    pub active: String,
    pub locked: bool,
    pub layout: Option<Node>,
    pub saved: Vec<Saved>,
    #[serde(default)]
    pub floating: Vec<Floating>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 2,
            active: "b-default".into(),
            locked: false,
            layout: Self::builtin("b-default"),
            saved: vec![],
            floating: vec![],
        }
    }
}
impl Workspace {
    pub const BUILTINS: [(&'static str, &'static str); 4] = [
        ("b-default", "기본"),
        ("b-prep", "예배 준비 (편집 크게)"),
        ("b-live", "예배 중 (송출 크게)"),
        ("b-simple", "간단 (큐시트 + LIVE)"),
    ];
    fn builtin(id: &str) -> Option<Node> {
        use Pane::*;
        Some(if id == "b-simple" {
            Node::split(true, 0.3, Node::Pane(Cue), Node::tabs(vec![Live, Main]))
        } else {
            let prep = id == "b-prep";
            let live = id == "b-live";
            Node::split(
                true,
                0.2,
                Node::Pane(Cue),
                Node::split(
                    true,
                    if prep {
                        0.72
                    } else if live {
                        0.4
                    } else {
                        0.55
                    },
                    Node::split(
                        false,
                        if prep { 0.55 } else { 0.6 },
                        Node::Pane(Main),
                        Node::tabs(if prep {
                            vec![Library, Quick, Stage]
                        } else if live {
                            vec![Stage, Library, Quick]
                        } else {
                            vec![Quick, Library, Stage, Media]
                        }),
                    ),
                    Node::split(
                        false,
                        if live { 0.58 } else { 0.5 },
                        Node::Pane(if live { Live } else { Preview }),
                        Node::Pane(if live { Preview } else { Live }),
                    ),
                ),
            )
        })
    }
    fn valid_layout(n: &Option<Node>, floating: &[Floating]) -> bool {
        let mut seen = vec![];
        n.as_ref().is_some_and(|n| n.valid(&mut seen, 0))
            && floating
                .iter()
                .all(|f| f.valid(&mut seen))
    }
    pub fn load(settings: &Value) -> Self {
        let mut w = serde_json::from_value::<Self>(settings["rsWorkspace"].clone())
            .ok()
            .filter(|w| {
                (w.version == 1 || w.version == 2)
                    && w.saved.len() <= 30
                    && Self::valid_layout(&w.layout, &w.floating)
                    && w.saved.iter().all(|s| {
                        !s.id.starts_with("b-")
                            && !s.id.is_empty()
                            && !s.name.trim().is_empty()
                            && Self::valid_layout(&s.layout, &s.floating)
                    })
            })
            .unwrap_or_default();
        w.version = 2;
        if !Self::BUILTINS.iter().any(|(id, _)| *id == w.active)
            && !w.saved.iter().any(|s| s.id == w.active)
        {
            w.active = "b-default".into()
        }
        if settings["rsWorkspace"].is_null() {
            w.locked = settings["dockLocked"].as_bool().unwrap_or(false)
        }
        w
    }
    pub fn move_to(&mut self, p: Pane, target: Pane, side: u8) {
        if self.locked
            || p == target
            || side > 4
            || !self.layout.as_ref().is_some_and(|n| n.contains(target))
        {
            return;
        }
        self.remove_floating(p);
        self.layout = self.layout.take().and_then(|n| n.remove(p));
        if let Some(n) = &mut self.layout {
            n.insert(p, target, side)
        }
    }
    pub fn visible(&self, p: Pane) -> bool {
        self.layout.as_ref().is_some_and(|n| n.contains(p))
            || self.floating.iter().any(|f| f.contains(p))
    }
    pub fn toggle(&mut self, p: Pane) {
        if self.locked {
            return;
        }
        if self.visible(p) {
            if self
                .layout
                .as_ref()
                .is_some_and(|n| n.contains(p) && n.count() == 1)
            {
                return;
            }
            self.layout = self.layout.take().and_then(|n| n.remove(p));
            self.remove_floating(p)
        } else {
            self.attach(p)
        }
    }
    pub fn attach(&mut self, p: Pane) {
        if self.locked {
            return;
        }
        self.remove_floating(p);
        if self.layout.as_ref().is_some_and(|n| n.contains(p)) {
            return;
        }
        self.layout = Some(match self.layout.take() {
            Some(n) => Node::split(true, 0.65, n, Node::Pane(p)),
            None => Node::Pane(p),
        })
    }
    pub fn float(&mut self, p: Pane) {
        if self.locked
            || !self
                .layout
                .as_ref()
                .is_some_and(|n| n.contains(p) && n.count() > 1)
        {
            return;
        }
        self.layout = self.layout.take().and_then(|n| n.remove(p));
        self.floating.push(Floating { pane: p, group: None, position: None, size: None })
    }
    fn remove_floating(&mut self, p: Pane) {
        self.floating = self.floating.drain(..).filter_map(|mut f| {
            if !f.contains(p) { return Some(f); }
            let node = f.node().remove(p)?;
            f.pane = match &node { Node::Pane(p) => *p, Node::Tabs { panes, .. } => panes[0], _ => unreachable!() };
            f.group = Some(node);
            Some(f)
        }).collect();
    }
    pub fn group(&self, p: Pane) -> Option<Node> {
        self.layout.as_ref().and_then(|n| n.group(p))
            .or_else(|| self.floating.iter().find(|f| f.contains(p)).map(Floating::node))
    }
    pub fn float_group(&mut self, p: Pane) {
        if self.locked { return; }
        let Some(group) = self.layout.as_ref().and_then(|n| n.group(p)) else { return; };
        if self.layout.as_ref().unwrap().count() <= group.count() { return; }
        for tab in Pane::ALL.into_iter().filter(|t| group.contains(*t)) {
            self.layout = self.layout.take().and_then(|n| n.remove(tab));
        }
        let group = match group { Node::Tabs { panes, active, .. } => Node::Tabs { panes, active, collapsed: false }, n => n };
        self.floating.push(Floating { pane: p, group: Some(group), position: None, size: None });
    }
    pub fn attach_group(&mut self, p: Pane) {
        if self.locked { return; }
        let Some(i) = self.floating.iter().position(|f| f.contains(p)) else { return; };
        let node = self.floating.remove(i).node();
        self.layout = Some(match self.layout.take() {
            Some(n) => Node::split(true, 0.65, n, node), None => node,
        });
    }
    pub fn merge_group(&mut self, p: Pane, target: Pane) {
        if self.locked || !self.visible(target) { return; }
        let Some(group) = self.group(p) else { return; };
        if group.contains(target) { return; }
        let active = match &group { Node::Tabs { active, .. } => *active, _ => p };
        let panes: Vec<_> = match group { Node::Tabs { panes, .. } => panes, _ => vec![p] };
        for tab in &panes {
            self.layout = self.layout.take().and_then(|n| n.remove(*tab));
            self.remove_floating(*tab);
        }
        for tab in panes.into_iter().filter(|t| *t != active).chain(std::iter::once(active)) {
            if let Some(n) = &mut self.layout { n.insert(tab, target, 4); }
            for f in &mut self.floating {
                if f.contains(target) { let mut n = f.node(); n.insert(tab, target, 4); f.group = Some(n); }
            }
        }
    }
    pub fn quick_rendered(&self) -> bool {
        self.layout.as_ref().is_some_and(|n| n.rendered(Pane::Quick))
            || self.floating.iter().any(|f| f.node().rendered(Pane::Quick))
    }
    pub fn save_as(&mut self, name: &str) {
        if name.trim().is_empty() || self.saved.len() >= 30 {
            return;
        }
        let mut id = format!("w{}", crate::util::now_ms());
        while self.saved.iter().any(|s| s.id == id) {
            id.push('x')
        }
        self.saved.push(Saved {
            id: id.clone(),
            name: name.trim().chars().take(40).collect(),
            layout: self.layout.clone(),
            floating: self.floating.clone(),
        });
        self.active = id
    }
    pub fn select(&mut self, id: &str) {
        if Self::BUILTINS.iter().any(|(x, _)| *x == id) {
            self.layout = Self::builtin(id);
            self.floating.clear();
            self.active = id.into()
        } else if let Some(s) = self.saved.iter().find(|s| s.id == id) {
            self.layout = s.layout.clone();
            self.floating = s.floating.clone();
            self.active = id.into()
        }
    }
    pub fn delete(&mut self, id: &str) {
        self.saved.retain(|s| s.id != id);
        if self.active == id {
            self.select("b-default")
        }
    }
    pub fn move_up(&mut self, id: &str) {
        if let Some(i) = self.saved.iter().position(|s| s.id == id) {
            if i > 0 {
                self.saved.swap(i, i - 1)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_roundtrip_and_original() {
        let mut w = Workspace::default();
        w.move_to(Pane::Cue, Pane::Live, 4);
        w.float(Pane::Quick);
        w.save_as("예배");
        let mut s = serde_json::json!({"dockLayout":{"old":1},"workspaces":[1],"rsWorkspace":w});
        let before = s.clone();
        assert_eq!(Workspace::load(&s), w);
        s["rsWorkspace"] = serde_json::to_value(&w).unwrap();
        assert_eq!(s["dockLayout"], before["dockLayout"]);
        assert_eq!(s["workspaces"], before["workspaces"]);
    }
    #[test]
    fn workspace_migrate_v1() {
        let s = serde_json::json!({"rsWorkspace":{"version":1,"active":"b-default","locked":false,"layout":{"Pane":"Bible"},"saved":[]}});
        let w = Workspace::load(&s);
        assert_eq!(w.version, 2);
        assert!(w.visible(Pane::Main));
    }
    #[test]
    fn workspace_last_panel_lock_restore() {
        let mut w = Workspace::default();
        for p in Pane::ALL {
            w.toggle(p)
        }
        assert_eq!(w.layout.as_ref().unwrap().count(), 1);
        for p in Pane::ALL {
            if !w.visible(p) {
                w.toggle(p)
            }
        }
        assert!(Pane::ALL.iter().all(|p| w.visible(*p)));
        w.locked = true;
        let old = w.clone();
        w.float(Pane::Cue);
        w.toggle(Pane::Cue);
        w.move_to(Pane::Cue, Pane::Live, 4);
        assert_eq!(old, w);
    }
    #[test]
    fn workspace_invalid() {
        let mut w = Workspace::default();
        w.layout = Some(Node::tabs(vec![Pane::Cue, Pane::Cue]));
        assert_eq!(
            Workspace::load(&serde_json::json!({"rsWorkspace":w})),
            Workspace::default()
        );
        w.layout = None;
        assert_eq!(
            Workspace::load(&serde_json::json!({"rsWorkspace":w})),
            Workspace::default()
        );
    }
    #[test]
    fn workspace_presets_management() {
        let mut w = Workspace::default();
        for (id, _) in Workspace::BUILTINS {
            w.select(id);
            assert_eq!(w.active, id);
            assert!(Workspace::valid_layout(&w.layout, &w.floating));
        }
        w.save_as("一");
        let a = w.active.clone();
        w.save_as("二");
        let b = w.active.clone();
        w.move_up(&b);
        assert_eq!(w.saved[0].id, b);
        w.delete(&b);
        w.select(&a);
        assert_eq!(w.active, a);
    }

    #[test]
    fn workspace_group_float_merge_geometry() {
        let mut w = Workspace::default();
        let original = w.group(Pane::Quick).unwrap();
        w.float_group(Pane::Quick);
        assert_eq!(w.floating.len(), 1);
        assert_eq!(w.floating[0].node(), original);
        w.floating[0].position = Some([-800.0, 42.0]);
        w.floating[0].size = Some([640.0, 480.0]);
        w.save_as("창 배치");
        assert_eq!(Workspace::load(&serde_json::json!({"rsWorkspace":w})), w);
        w.merge_group(Pane::Quick, Pane::Live);
        assert!(w.floating.is_empty());
        assert!(w.group(Pane::Live).unwrap().contains(Pane::Library));
        assert!(Workspace::valid_layout(&w.layout, &w.floating));
        w.float_group(Pane::Live);
        w.attach_group(Pane::Live);
        assert!(w.floating.is_empty());
        assert!(Workspace::valid_layout(&w.layout, &w.floating));
    }
    #[test]
    fn workspace_fold_resize_and_quick_fallback() {
        let mut a = Node::tabs(vec![Pane::Quick, Pane::Library]);
        let b = Node::Pane(Pane::Live);
        let mut ratio = 0.63;
        assert!(a.rendered(Pane::Quick));
        if let Node::Tabs { collapsed, .. } = &mut a { *collapsed = true; }
        Node::resize_ratio(&mut ratio, &a, &b, 0.1);
        assert_eq!(ratio, 0.63);
        assert!(!a.rendered(Pane::Quick));
        if let Node::Tabs { collapsed, .. } = &mut a { *collapsed = false; }
        Node::resize_ratio(&mut ratio, &a, &b, 0.4);
        assert_eq!(ratio, 0.4);
        let mut w = Workspace::default();
        assert!(w.quick_rendered());
        w.float_group(Pane::Quick);
        assert!(w.quick_rendered());
        w.toggle(Pane::Quick);
        assert!(!w.quick_rendered());
        assert!(w.visible(Pane::Library));
    }
    #[test]
    fn workspace_group_lock_and_legacy_floating() {
        let mut w = Workspace::default();
        w.float(Pane::Quick);
        let legacy = serde_json::json!({"rsWorkspace": {"version":2,"active":"b-default","locked":false,"layout":w.layout,"saved":[],"floating":[{"pane":"Quick"}]}});
        assert!(Workspace::load(&legacy).visible(Pane::Quick));
        w.locked = true;
        let old = w.clone();
        w.float_group(Pane::Cue);
        w.merge_group(Pane::Quick, Pane::Live);
        w.attach_group(Pane::Quick);
        assert_eq!(w, old);
        w.locked = false;
        w.merge_group(Pane::Live, Pane::Quick);
        assert!(w.floating[0].node().contains(Pane::Live));
        assert!(Workspace::valid_layout(&w.layout, &w.floating));
    }

    #[test]
    fn workspace_group_merge_main_and_invalid_geometry() {
        let mut w = Workspace::default();
        w.merge_group(Pane::Quick, Pane::Preview);
        let group = w.group(Pane::Preview).unwrap();
        assert!(group.contains(Pane::Library));
        assert!(group.contains(Pane::Media));
        assert!(Workspace::valid_layout(&w.layout, &w.floating));
        let before = w.clone();
        w.merge_group(Pane::Quick, Pane::Library);
        assert_eq!(w, before);
        w.float_group(Pane::Quick);
        w.floating[0].size = Some([-1.0, 400.0]);
        assert_eq!(Workspace::load(&serde_json::json!({"rsWorkspace":w})), Workspace::default());
    }
}
