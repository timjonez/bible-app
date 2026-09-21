use crate::nav::Ref;

/// Identity of a tab in the workspace. Stable for the tab's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

impl TabId {
    pub fn keyword(self) -> String {
        self.0.to_string()
    }

    pub fn from_keyword(s: impl AsRef<str>) -> Option<Self> {
        s.as_ref().parse().ok().map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Left,
    Right,
}

impl Pane {
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarksPage {
    Bookmarks,
    Notes,
}

impl MarksPage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bookmarks => "bookmarks",
            Self::Notes => "notes",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s == "notes" {
            Self::Notes
        } else {
            Self::Bookmarks
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    Passage {
        at: Ref,
    },
    Mhc {
        at: Ref,
        follow: bool,
    },
    Tsk {
        at: Ref,
        follow: bool,
    },
    Library {
        module: String,
        headword: Option<String>,
    },
    Marks {
        page: MarksPage,
    },
    Occurrences {
        code: String,
    },
}

impl TabKind {
    pub fn is_passage(&self) -> bool {
        matches!(self, Self::Passage { .. })
    }

    pub fn is_mhc(&self) -> bool {
        matches!(self, Self::Mhc { .. })
    }

    pub fn is_tsk(&self) -> bool {
        matches!(self, Self::Tsk { .. })
    }

    pub fn follows_verse(&self) -> bool {
        match self {
            Self::Mhc { follow, .. } | Self::Tsk { follow, .. } => *follow,
            _ => false,
        }
    }

    pub fn at(&self) -> Option<Ref> {
        match self {
            Self::Passage { at } | Self::Mhc { at, .. } | Self::Tsk { at, .. } => Some(*at),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    pub pane: Pane,
    pub detached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenResult {
    pub id: TabId,
    pub created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    Closed,
    /// Last passage was closed; a replacement passage was created.
    Replaced {
        id: TabId,
        at: Ref,
    },
}

#[derive(Debug, Clone)]
pub struct Workspace {
    next_id: u64,
    tabs: Vec<Tab>,
    focused: TabId,
    last_passage: TabId,
    last_at: Ref,
    left_sel: TabId,
    right_sel: Option<TabId>,
}

impl Workspace {
    pub fn new(start: Ref) -> Self {
        let id = TabId(1);
        Self {
            next_id: 2,
            tabs: vec![Tab {
                id,
                kind: TabKind::Passage { at: start },
                pane: Pane::Left,
                detached: false,
            }],
            focused: id,
            last_passage: id,
            last_at: start,
            left_sel: id,
            right_sel: None,
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn focused(&self) -> TabId {
        self.focused
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn focused_tab(&self) -> Option<&Tab> {
        self.tab(self.focused)
    }

    pub fn last_at(&self) -> Ref {
        self.last_at
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_split(&self) -> bool {
        self.tabs
            .iter()
            .any(|t| !t.detached && t.pane == Pane::Right)
    }

    pub fn focused_passage(&self) -> Option<&Tab> {
        let focused = self.tab(self.focused)?;
        if focused.kind.is_passage() {
            return Some(focused);
        }
        self.tab(self.last_passage)
            .filter(|t| t.kind.is_passage())
            .or_else(|| self.tabs.iter().find(|t| t.kind.is_passage()))
    }

    pub fn focused_passage_id(&self) -> Option<TabId> {
        self.focused_passage().map(|t| t.id)
    }

    pub fn focused_passage_ref(&self) -> Option<Ref> {
        self.focused_passage().and_then(|t| t.kind.at())
    }

    /// Visible selected tabs: one, or two when split.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible(&self) -> Vec<TabId> {
        let mut ids = Vec::with_capacity(2);
        if self.tab(self.left_sel).is_some_and(|t| !t.detached) {
            ids.push(self.left_sel);
        }
        if self.is_split() {
            if let Some(id) = self.right_sel {
                if self.tab(id).is_some_and(|t| !t.detached) && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        if ids.is_empty() {
            if let Some(t) = self.tabs.iter().find(|t| !t.detached) {
                ids.push(t.id);
            }
        }
        ids
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible_passage_refs(&self) -> Vec<Ref> {
        self.visible()
            .into_iter()
            .filter_map(|id| {
                self.tab(id).and_then(|t| match t.kind {
                    TabKind::Passage { at } => Some(at),
                    _ => None,
                })
            })
            .collect()
    }

    pub fn has_mhc(&self) -> bool {
        self.find_kind(|k| k.is_mhc()).is_some()
    }

    pub fn has_tsk(&self) -> bool {
        self.find_kind(|k| k.is_tsk()).is_some()
    }

    pub fn find_kind(&self, pred: impl Fn(&TabKind) -> bool) -> Option<TabId> {
        self.tabs.iter().find(|t| pred(&t.kind)).map(|t| t.id)
    }

    pub fn focus(&mut self, id: TabId) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let pane = tab.pane;
        let detached = tab.detached;
        let is_passage = tab.kind.is_passage();
        let at = tab.kind.at();
        self.focused = id;
        if is_passage {
            self.last_passage = id;
            if let Some(at) = at {
                self.last_at = at;
            }
        }
        if !detached {
            match pane {
                Pane::Left => self.left_sel = id,
                Pane::Right => self.right_sel = Some(id),
            }
        }
    }

    pub fn set_host(&mut self, id: TabId, pane: Pane, detached: bool) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        tab.pane = pane;
        tab.detached = detached;
        if !detached {
            match pane {
                Pane::Left => self.left_sel = id,
                Pane::Right => self.right_sel = Some(id),
            }
        }
        if detached {
            self.prune_sel(id);
        }
        self.repair_sel();
    }

    pub fn navigate_passage(&mut self, id: TabId, at: Ref) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        if let TabKind::Passage { at: slot } = &mut tab.kind {
            *slot = at;
        } else {
            return;
        }
        self.last_passage = id;
        self.last_at = at;
        self.follow_verse(at);
    }

    pub fn set_passage_verse(&mut self, id: TabId, verse: u8) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        let TabKind::Passage { at } = &mut tab.kind else {
            return;
        };
        at.verse = verse;
        let at = *at;
        self.last_at = at;
        self.follow_verse(at);
    }

    /// Update MHC/TSK tabs that have follow-verse on.
    pub fn follow_verse(&mut self, at: Ref) {
        for tab in &mut self.tabs {
            match &mut tab.kind {
                TabKind::Mhc {
                    at: slot, follow, ..
                }
                | TabKind::Tsk {
                    at: slot, follow, ..
                } if *follow => *slot = at,
                _ => {}
            }
        }
    }

    pub fn set_follow(&mut self, id: TabId, follow: bool) {
        let last_at = self.last_at;
        if let Some(tab) = self.tab_mut(id) {
            match &mut tab.kind {
                TabKind::Mhc {
                    follow: slot, at, ..
                }
                | TabKind::Tsk {
                    follow: slot, at, ..
                } => {
                    *slot = follow;
                    if follow {
                        *at = last_at;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn open_passage(&mut self, at: Ref) -> OpenResult {
        let pane = self.insert_pane();
        let id = self.add_tab(TabKind::Passage { at }, pane, false);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    /// New passage in the other pane (creates the one allowed split).
    pub fn open_passage_beside(&mut self, from: TabId, at: Ref) -> OpenResult {
        let pane = self
            .tab(from)
            .filter(|t| !t.detached)
            .map(|t| t.pane.other())
            .unwrap_or(Pane::Right);
        let id = self.add_tab(TabKind::Passage { at }, pane, false);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    pub fn open_mhc(&mut self, at: Ref) -> OpenResult {
        self.open_unique(
            |k| k.is_mhc(),
            TabKind::Mhc { at, follow: true },
            |kind| {
                if let TabKind::Mhc { at: slot, .. } = kind {
                    *slot = at;
                }
            },
        )
    }

    pub fn open_tsk(&mut self, at: Ref) -> OpenResult {
        self.open_unique(
            |k| k.is_tsk(),
            TabKind::Tsk { at, follow: true },
            |kind| {
                if let TabKind::Tsk { at: slot, .. } = kind {
                    *slot = at;
                }
            },
        )
    }

    pub fn open_library(&mut self, module: String, headword: Option<String>) -> OpenResult {
        self.open_unique(
            |k| matches!(k, TabKind::Library { .. }),
            TabKind::Library {
                module: module.clone(),
                headword: headword.clone(),
            },
            |kind| {
                if let TabKind::Library {
                    module: m,
                    headword: h,
                } = kind
                {
                    *m = module.clone();
                    *h = headword.clone();
                }
            },
        )
    }

    pub fn open_marks(&mut self, page: MarksPage) -> OpenResult {
        self.open_unique(
            |k| matches!(k, TabKind::Marks { .. }),
            TabKind::Marks { page },
            |kind| {
                if let TabKind::Marks { page: slot } = kind {
                    *slot = page;
                }
            },
        )
    }

    pub fn open_occurrences(&mut self, code: String) -> OpenResult {
        self.open_unique(
            |k| matches!(k, TabKind::Occurrences { .. }),
            TabKind::Occurrences { code: code.clone() },
            |kind| {
                if let TabKind::Occurrences { code: slot } = kind {
                    *slot = code.clone();
                }
            },
        )
    }

    /// Move `id` into the other main pane, creating a split if needed.
    /// Returns false if there is nothing to split against.
    pub fn move_beside(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        if tab.detached {
            let dest = if self
                .tabs
                .iter()
                .any(|t| !t.detached && t.pane == Pane::Right)
            {
                Pane::Left
            } else {
                Pane::Right
            };
            self.set_host(id, dest, false);
            self.focus(id);
            return true;
        }
        let src = tab.pane;
        let dest = src.other();
        let others = self
            .tabs
            .iter()
            .filter(|t| !t.detached && t.pane == src && t.id != id)
            .count();
        if others == 0 && !self.tabs.iter().any(|t| !t.detached && t.pane == dest) {
            return false;
        }
        self.set_host(id, dest, false);
        self.focus(id);
        if self.sel_in(src) == Some(id) {
            if let Some(other) = self.first_in(src) {
                match src {
                    Pane::Left => self.left_sel = other,
                    Pane::Right => self.right_sel = Some(other),
                }
            }
        }
        true
    }

    pub fn detach(&mut self, id: TabId) {
        self.set_host(id, Pane::Left, true);
    }

    pub fn close(&mut self, id: TabId) -> CloseOutcome {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return CloseOutcome::Closed;
        };
        let tab = self.tabs.remove(idx);
        self.prune_sel(id);

        let passages = self.tabs.iter().filter(|t| t.kind.is_passage()).count();
        if passages == 0 {
            let at = tab.kind.at().unwrap_or(self.last_at);
            let new_id = self.add_tab(TabKind::Passage { at }, Pane::Left, false);
            self.last_passage = new_id;
            self.last_at = at;
            return CloseOutcome::Replaced { id: new_id, at };
        }

        if self.focused == id {
            let next = if self.tab(self.last_passage).is_some() {
                self.last_passage
            } else {
                self.tabs
                    .iter()
                    .find(|t| t.kind.is_passage())
                    .or_else(|| self.tabs.first())
                    .map(|t| t.id)
                    .unwrap_or(id)
            };
            self.focus(next);
        }
        self.repair_sel();
        CloseOutcome::Closed
    }

    fn open_unique(
        &mut self,
        pred: impl Fn(&TabKind) -> bool,
        kind: TabKind,
        update: impl FnOnce(&mut TabKind),
    ) -> OpenResult {
        if let Some(id) = self.find_kind(pred) {
            if let Some(tab) = self.tab_mut(id) {
                update(&mut tab.kind);
            }
            self.focus(id);
            OpenResult { id, created: false }
        } else {
            let pane = self.insert_pane();
            let id = self.add_tab(kind, pane, false);
            OpenResult { id, created: true }
        }
    }

    fn add_tab(&mut self, kind: TabKind, pane: Pane, detached: bool) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            kind,
            pane,
            detached,
        });
        self.focus(id);
        id
    }

    fn insert_pane(&self) -> Pane {
        self.tab(self.focused)
            .filter(|t| !t.detached)
            .map(|t| t.pane)
            .unwrap_or(Pane::Left)
    }

    fn first_in(&self, pane: Pane) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| !t.detached && t.pane == pane)
            .map(|t| t.id)
    }

    fn sel_in(&self, pane: Pane) -> Option<TabId> {
        match pane {
            Pane::Left => Some(self.left_sel),
            Pane::Right => self.right_sel,
        }
    }

    fn prune_sel(&mut self, id: TabId) {
        if self.left_sel == id {
            if let Some(other) = self.first_in(Pane::Left).filter(|o| *o != id) {
                self.left_sel = other;
            }
        }
        if self.right_sel == Some(id) {
            self.right_sel = self.first_in(Pane::Right).filter(|o| *o != id);
        }
    }

    fn repair_sel(&mut self) {
        if self
            .tab(self.left_sel)
            .is_none_or(|t| t.detached || t.pane != Pane::Left)
        {
            if let Some(id) = self.first_in(Pane::Left) {
                self.left_sel = id;
            }
        }
        if self
            .right_sel
            .and_then(|id| self.tab(id))
            .is_none_or(|t| t.detached || t.pane != Pane::Right)
        {
            self.right_sel = self.first_in(Pane::Right);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(book: u8, chapter: u8, verse: u8) -> Ref {
        Ref {
            book,
            chapter,
            verse,
        }
    }

    fn start() -> Workspace {
        Workspace::new(r(1, 1, 1))
    }

    #[test]
    fn first_launch_is_one_passage_no_split() {
        let ws = start();
        assert_eq!(ws.tabs().len(), 1);
        assert!(!ws.is_split());
        assert_eq!(ws.visible().len(), 1);
        assert_eq!(ws.focused_passage_ref(), Some(r(1, 1, 1)));
        assert!(ws.tabs()[0].kind.is_passage());
    }

    #[test]
    fn extra_passage_tabs_are_allowed() {
        let mut ws = start();
        let first = ws.focused();
        let second = ws.open_passage(r(43, 3, 16));
        assert!(second.created);
        assert_ne!(second.id, first);
        assert_eq!(ws.tabs().len(), 2);
        assert!(!ws.is_split());
        assert_eq!(ws.focused(), second.id);
        assert_eq!(ws.focused_passage_ref(), Some(r(43, 3, 16)));
    }

    #[test]
    fn open_mhc_reuses_the_same_tab() {
        let mut ws = start();
        let a = ws.open_mhc(r(1, 1, 1));
        assert!(a.created);
        let b = ws.open_mhc(r(1, 2, 4));
        assert!(!b.created);
        assert_eq!(a.id, b.id);
        assert_eq!(ws.tabs().len(), 2);
        assert_eq!(ws.tab(a.id).and_then(|t| t.kind.at()), Some(r(1, 2, 4)));
        assert!(ws.has_mhc());
        let c = ws.open_tsk(r(1, 1, 1));
        assert!(c.created);
        let d = ws.open_tsk(r(43, 1, 1));
        assert!(!d.created);
        assert_eq!(c.id, d.id);
        assert_eq!(ws.tabs().iter().filter(|t| t.kind.is_tsk()).count(), 1);
    }

    #[test]
    fn library_marks_and_occurrences_reuse() {
        let mut ws = start();
        let lib = ws.open_library("Easton".into(), Some("God".into()));
        let lib2 = ws.open_library("Webster".into(), Some("Divide".into()));
        assert_eq!(lib.id, lib2.id);
        match &ws.tab(lib.id).unwrap().kind {
            TabKind::Library { module, headword } => {
                assert_eq!(module, "Webster");
                assert_eq!(headword.as_deref(), Some("Divide"));
            }
            other => panic!("expected library, got {other:?}"),
        }
        let marks = ws.open_marks(MarksPage::Bookmarks);
        let notes = ws.open_marks(MarksPage::Notes);
        assert_eq!(marks.id, notes.id);
        assert!(matches!(
            ws.tab(marks.id).unwrap().kind,
            TabKind::Marks {
                page: MarksPage::Notes
            }
        ));
        let occ = ws.open_occurrences("H430".into());
        let occ2 = ws.open_occurrences("G26".into());
        assert_eq!(occ.id, occ2.id);
        match &ws.tab(occ.id).unwrap().kind {
            TabKind::Occurrences { code } => assert_eq!(code, "G26"),
            other => panic!("expected occurrences, got {other:?}"),
        }
    }

    #[test]
    fn split_holds_two_refs() {
        let mut ws = start();
        let left = ws.focused();
        let right = ws.open_passage_beside(left, r(43, 3, 16));
        assert!(ws.is_split());
        assert_eq!(ws.visible().len(), 2);
        let refs = ws.visible_passage_refs();
        assert!(refs.contains(&r(1, 1, 1)));
        assert!(refs.contains(&r(43, 3, 16)));
        assert_eq!(ws.tab(right.id).unwrap().pane, Pane::Right);
        assert_eq!(ws.tab(left).unwrap().pane, Pane::Left);
        assert_eq!(ws.focused(), right.id);
    }

    #[test]
    fn focus_switches_header_passage() {
        let mut ws = start();
        let a = ws.focused();
        let b = ws.open_passage(r(19, 23, 1)).id;
        assert_eq!(ws.focused_passage_ref(), Some(r(19, 23, 1)));
        ws.focus(a);
        assert_eq!(ws.focused(), a);
        assert_eq!(ws.focused_passage_ref(), Some(r(1, 1, 1)));
        ws.open_mhc(r(1, 1, 1));
        assert!(!ws.focused_tab().unwrap().kind.is_passage());
        assert_eq!(ws.focused_passage_id(), Some(a));
        assert_eq!(ws.focused_passage_ref(), Some(r(1, 1, 1)));
        let _ = b;
    }

    #[test]
    fn follow_verse_updates_mhc_and_tsk_only() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        let lib = ws.open_library("Easton".into(), Some("God".into())).id;
        let marks = ws.open_marks(MarksPage::Notes).id;
        let occ = ws.open_occurrences("H430".into()).id;
        ws.navigate_passage(passage, r(43, 3, 16));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(43, 3, 16)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(43, 3, 16)));
        match &ws.tab(lib).unwrap().kind {
            TabKind::Library { headword, .. } => assert_eq!(headword.as_deref(), Some("God")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            ws.tab(marks).unwrap().kind,
            TabKind::Marks {
                page: MarksPage::Notes
            }
        ));
        match &ws.tab(occ).unwrap().kind {
            TabKind::Occurrences { code } => assert_eq!(code, "H430"),
            other => panic!("{other:?}"),
        }
        ws.set_follow(mhc, false);
        ws.navigate_passage(passage, r(1, 2, 3));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(43, 3, 16)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(1, 2, 3)));
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(ws.tab(tsk).unwrap().kind.follows_verse());
    }

    #[test]
    fn menu_open_mhc_updates_even_when_follow_is_off() {
        let mut ws = start();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        ws.set_follow(mhc, false);
        ws.open_mhc(r(19, 23, 1));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(19, 23, 1)));
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
    }

    #[test]
    fn move_beside_splits_stacked_tabs() {
        let mut ws = start();
        let a = ws.focused();
        let b = ws.open_passage(r(43, 3, 16)).id;
        assert!(!ws.is_split());
        assert!(ws.move_beside(b));
        assert!(ws.is_split());
        assert_eq!(ws.tab(a).unwrap().pane, Pane::Left);
        assert_eq!(ws.tab(b).unwrap().pane, Pane::Right);
        assert_eq!(ws.visible_passage_refs().len(), 2);
    }

    #[test]
    fn cannot_split_the_only_tab() {
        let mut ws = start();
        let a = ws.focused();
        assert!(!ws.move_beside(a));
        assert!(!ws.is_split());
    }

    #[test]
    fn close_study_keeps_passage() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert_eq!(ws.close(mhc), CloseOutcome::Closed);
        assert!(!ws.has_mhc());
        assert_eq!(ws.tabs().len(), 1);
        assert_eq!(ws.focused(), passage);
    }

    #[test]
    fn closing_last_passage_replaces_it() {
        let mut ws = start();
        ws.navigate_passage(ws.focused(), r(43, 3, 16));
        let id = ws.focused();
        match ws.close(id) {
            CloseOutcome::Replaced { id: new_id, at } => {
                assert_ne!(new_id, id);
                assert_eq!(at, r(43, 3, 16));
                assert_eq!(ws.tabs().len(), 1);
                assert!(ws.tabs()[0].kind.is_passage());
            }
            other => panic!("expected replacement, got {other:?}"),
        }
    }
}
