//! Rust 작업공간은 rsWorkspace에 분리 저장한다. Electron dockview JSON은 손대지 않는다.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pane {
    Cue,
    Bible,
    Preview,
    Live,
}
impl Pane {
    pub const ALL: [Self; 4] = [Self::Cue, Self::Bible, Self::Preview, Self::Live];
    pub fn title(self) -> &'static str {
        match self {
            Self::Cue => "큐시트 · 라이브러리",
            Self::Bible => "성경 · 본문",
            Self::Preview => "미리보기",
            Self::Live => "LIVE",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Pane(Pane),
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
    pub fn contains(&self, p: Pane) -> bool {
        match self {
            Self::Pane(x) => *x == p,
            Self::Split { a, b, .. } => a.contains(p) || b.contains(p),
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
            Self::Pane(t) if *t == target => {
                let old = self.clone();
                let new = Self::Pane(p);
                *self = if side == 0 || side == 2 {
                    Self::split(side < 2, 0.5, new, old)
                } else {
                    Self::split(side < 2, 0.5, old, new)
                };
            }
            Self::Split { a, b, .. } => {
                if a.contains(target) {
                    a.insert(p, target, side)
                } else {
                    b.insert(p, target, side)
                }
            }
            _ => {}
        }
    }
    fn valid(&self, seen: &mut Vec<Pane>, depth: usize) -> bool {
        if depth > 8 {
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
            Self::Split { ratio, a, b, .. } => {
                ratio.is_finite()
                    && *ratio >= 0.08
                    && *ratio <= 0.92
                    && a.valid(seen, depth + 1)
                    && b.valid(seen, depth + 1)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub name: String,
    pub layout: Option<Node>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u8,
    pub active: String,
    pub locked: bool,
    pub layout: Option<Node>,
    pub saved: Vec<Saved>,
}
impl Default for Workspace {
    fn default() -> Self {
        use Pane::*;
        Self {
            version: 1,
            active: "b-default".into(),
            locked: false,
            layout: Some(Node::split(
                true,
                0.28,
                Node::Pane(Cue),
                Node::split(
                    false,
                    0.6,
                    Node::split(true, 0.5, Node::Pane(Preview), Node::Pane(Live)),
                    Node::Pane(Bible),
                ),
            )),
            saved: vec![],
        }
    }
}
impl Workspace {
    pub fn load(settings: &Value) -> Self {
        let mut w = serde_json::from_value::<Self>(settings["rsWorkspace"].clone())
            .ok()
            .filter(|w| {
                w.version == 1
                    && w.saved.len() <= 30
                    && w.layout.as_ref().is_none_or(|n| n.valid(&mut vec![], 0))
                    && w.saved
                        .iter()
                        .all(|s| s.layout.as_ref().is_none_or(|n| n.valid(&mut vec![], 0)))
            })
            .unwrap_or_default();
        if settings["rsWorkspace"].is_null() {
            w.locked = settings["dockLocked"].as_bool().unwrap_or(false)
        }
        w
    }
    pub fn move_to(&mut self, p: Pane, target: Pane, side: u8) {
        if self.locked || p == target || side > 3 {
            return;
        }
        if !self.layout.as_ref().is_some_and(|n| n.contains(target)) {
            return;
        }
        self.layout = self.layout.take().and_then(|n| n.remove(p));
        if let Some(n) = &mut self.layout {
            n.insert(p, target, side)
        }
    }
    pub fn visible(&self, p: Pane) -> bool {
        self.layout.as_ref().is_some_and(|n| n.contains(p))
    }
    pub fn toggle(&mut self, p: Pane) {
        if self.locked {
            return;
        }
        if self.visible(p) {
            self.layout = self.layout.take().and_then(|n| n.remove(p))
        } else {
            self.layout = Some(match self.layout.take() {
                Some(n) => Node::split(true, 0.65, n, Node::Pane(p)),
                None => Node::Pane(p),
            })
        }
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
        });
        self.active = id;
    }
    pub fn select(&mut self, id: &str) {
        if id == "b-default" {
            self.layout = Self::default().layout;
            self.active = id.into()
        } else if let Some(s) = self.saved.iter().find(|s| s.id == id) {
            self.layout = s.layout.clone();
            self.active = id.into()
        }
    }
    pub fn delete(&mut self, id: &str) {
        self.saved.retain(|s| s.id != id);
        if self.active == id {
            self.select("b-default")
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn move_hide_restore() {
        let mut w = Workspace::default();
        w.move_to(Pane::Cue, Pane::Live, 3);
        assert!(w.visible(Pane::Cue));
        w.toggle(Pane::Bible);
        assert!(!w.visible(Pane::Bible));
        w.save_as("예배");
        let data = serde_json::json!({"rsWorkspace":w});
        assert_eq!(Workspace::load(&data), w);
        w.toggle(Pane::Bible);
        let id = w.active.clone();
        w.select(&id);
        assert!(!w.visible(Pane::Bible));
        w.delete(&id);
        assert_eq!(w.layout, Workspace::default().layout);
    }
    #[test]
    fn lock_and_invalid() {
        let mut w = Workspace::default();
        w.locked = true;
        let before = w.clone();
        w.toggle(Pane::Cue);
        w.move_to(Pane::Cue, Pane::Live, 1);
        assert_eq!(w, before);
        assert_eq!(
            Workspace::load(&serde_json::json!({"rsWorkspace":{"version":99}})),
            Workspace::default()
        );
    }
    #[test]
    fn empty_and_original_preserved() {
        let mut w = Workspace::default();
        for p in Pane::ALL {
            w.toggle(p)
        }
        assert!(w.layout.is_none());
        w.toggle(Pane::Live);
        assert!(w.visible(Pane::Live));
        let s = serde_json::json!({"dockLayout":"original","workspaces":[{"id":"old"}],"dockLocked":true});
        assert!(Workspace::load(&s).locked);
        assert_eq!(s["dockLayout"], "original");
    }
}
