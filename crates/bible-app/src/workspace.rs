use crate::nav::Ref;
use std::cell::Cell;
use std::rc::Rc;

/// Identity of a tab in the workspace. Stable for the tab's lifetime.
/// A reader window. The first one is [`WindowId::MAIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    pub const MAIN: Self = Self(0);

    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

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
    Search,
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

    pub fn is_search(&self) -> bool {
        matches!(self, Self::Search)
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
    pub window: WindowId,
    pub pane: Pane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenResult {
    pub id: TabId,
    pub created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOutcome {
    /// A new passage was opened on the other side.
    Duplicated { id: TabId, at: Ref },
    /// The tab moved to the other side.
    Moved,
    /// The tab is already alone on its side of a split.
    AlreadyBeside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetachOutcome {
    pub window: WindowId,
    pub source: WindowId,
}

#[derive(Debug, Clone)]
struct WindowRec {
    id: WindowId,
    focused: TabId,
    left_sel: Option<TabId>,
    right_sel: Option<TabId>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    next_id: u64,
    next_window: Rc<Cell<u64>>,
    tabs: Vec<Tab>,
    windows: Vec<WindowRec>,
    focused: TabId,
    last_passage: TabId,
    last_at: Ref,
}

impl Workspace {
    pub fn new(start: Ref) -> Self {
        let id = TabId(1);
        Self {
            next_id: 2,
            next_window: Rc::new(Cell::new(1)),
            tabs: vec![Tab {
                id,
                kind: TabKind::Passage { at: start },
                window: WindowId::MAIN,
                pane: Pane::Left,
            }],
            windows: vec![WindowRec {
                id: WindowId::MAIN,
                focused: id,
                left_sel: Some(id),
                right_sel: None,
            }],
            focused: id,
            last_passage: id,
            last_at: start,
        }
    }

    /// Shared counter so a drag-out can reserve an id before the model hears about it.
    pub fn window_counter(&self) -> Rc<Cell<u64>> {
        self.next_window.clone()
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
        self.is_window_split(WindowId::MAIN)
    }

    pub fn is_window_split(&self, window: WindowId) -> bool {
        self.tabs
            .iter()
            .any(|t| t.window == window && t.pane == Pane::Right)
    }

    /// False when the tab is already the only one on its side of a split.
    pub fn can_split(&self, id: TabId) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        if !self.is_window_split(tab.window) {
            return true;
        }
        self.count(tab.window, Some(tab.pane)) > 1
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

    pub fn focused_passage_in(&self, window: WindowId) -> Option<TabId> {
        let rec = self.window(window)?;
        if self
            .tab(rec.focused)
            .is_some_and(|t| t.window == window && t.kind.is_passage())
        {
            return Some(rec.focused);
        }
        self.tabs
            .iter()
            .find(|t| t.window == window && t.kind.is_passage())
            .map(|t| t.id)
    }

    pub fn focused_passage_ref_in(&self, window: WindowId) -> Option<Ref> {
        self.focused_passage_in(window)
            .and_then(|id| self.tab(id))
            .and_then(|t| t.kind.at())
    }

    /// Visible selected tabs in the main window: one, or two when split.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible(&self) -> Vec<TabId> {
        self.visible_in(WindowId::MAIN)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible_in(&self, window: WindowId) -> Vec<TabId> {
        let Some(rec) = self.window(window) else {
            return Vec::new();
        };
        let mut ids = Vec::with_capacity(2);
        if let Some(id) = rec.left_sel {
            if self
                .tab(id)
                .is_some_and(|t| t.window == window && t.pane == Pane::Left)
            {
                ids.push(id);
            }
        }
        if self.is_window_split(window) {
            if let Some(id) = rec.right_sel {
                if self
                    .tab(id)
                    .is_some_and(|t| t.window == window && t.pane == Pane::Right)
                    && !ids.contains(&id)
                {
                    ids.push(id);
                }
            }
        }
        if ids.is_empty() {
            if let Some(t) = self.tabs.iter().find(|t| t.window == window) {
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

    pub fn has_search(&self, window: WindowId) -> bool {
        self.find_search(window).is_some()
    }

    pub fn find_search(&self, window: WindowId) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| t.window == window && t.kind.is_search())
            .map(|t| t.id)
    }

    pub fn find_kind(&self, pred: impl Fn(&TabKind) -> bool) -> Option<TabId> {
        self.tabs.iter().find(|t| pred(&t.kind)).map(|t| t.id)
    }

    pub fn focus(&mut self, id: TabId) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let window = tab.window;
        let pane = tab.pane;
        let is_passage = tab.kind.is_passage();
        let at = tab.kind.at();
        self.focused = id;
        if is_passage {
            self.last_passage = id;
            if let Some(at) = at {
                self.last_at = at;
            }
        }
        if let Some(rec) = self.window_mut(window) {
            rec.focused = id;
            match pane {
                Pane::Left => rec.left_sel = Some(id),
                Pane::Right => rec.right_sel = Some(id),
            }
        }
    }

    /// Record where a tab landed after a tab view moved it.
    pub fn place(&mut self, id: TabId, window: WindowId, pane: Pane) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let source = tab.window;
        if source == window && tab.pane == pane {
            self.focus(id);
            return;
        }
        self.ensure_window(window);
        if let Some(tab) = self.tab_mut(id) {
            tab.window = window;
            tab.pane = pane;
        }
        self.repair(source);
        self.focus(id);
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn open_passage(&mut self, at: Ref) -> OpenResult {
        let (window, pane) = self.insert_slot();
        let id = self.add_tab(TabKind::Passage { at }, window, pane);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    pub fn open_passage_in(&mut self, window: WindowId, at: Ref) -> OpenResult {
        let pane = self.focused_pane(window);
        let id = self.add_tab(TabKind::Passage { at }, window, pane);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    /// New passage in the other pane of `from`'s window (creates the one allowed split).
    pub fn open_passage_beside(&mut self, from: TabId, at: Ref) -> OpenResult {
        let (window, pane) = self
            .tab(from)
            .map(|t| (t.window, t.pane.other()))
            .unwrap_or((WindowId::MAIN, Pane::Right));
        let id = self.add_tab(TabKind::Passage { at }, window, pane);
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

    /// One search tab in `window`. A second call focuses the one already there.
    pub fn open_search(&mut self, window: WindowId) -> OpenResult {
        if let Some(id) = self.find_search(window) {
            self.focus(id);
            return OpenResult { id, created: false };
        }
        let pane = self.focused_pane(window);
        let id = self.add_tab(TabKind::Search, window, pane);
        OpenResult { id, created: true }
    }

    /// Move `id` into the other pane of its window.
    /// Returns false when that would empty the only pane.
    pub fn move_beside(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        let window = tab.window;
        let src = tab.pane;
        let dest = src.other();
        let others = self
            .tabs
            .iter()
            .filter(|t| t.window == window && t.pane == src && t.id != id)
            .count();
        let dest_occupied = self
            .tabs
            .iter()
            .any(|t| t.window == window && t.pane == dest);
        if others == 0 && !dest_occupied {
            return false;
        }
        self.set_pane(id, dest);
        self.focus(id);
        true
    }

    pub fn split_tab(&mut self, id: TabId) -> SplitOutcome {
        let Some(tab) = self.tab(id).cloned() else {
            return SplitOutcome::AlreadyBeside;
        };
        let window = tab.window;
        let pane = tab.pane;
        if self.is_window_split(window) {
            if self.count(window, Some(pane)) <= 1 {
                return SplitOutcome::AlreadyBeside;
            }
            self.set_pane(id, pane.other());
            self.focus(id);
            return SplitOutcome::Moved;
        }
        if tab.kind.is_passage() {
            let at = tab.kind.at().unwrap_or(self.last_at);
            let new_id = self.add_tab(TabKind::Passage { at }, window, pane.other());
            self.last_passage = new_id;
            self.last_at = at;
            return SplitOutcome::Duplicated { id: new_id, at };
        }
        if self.count(window, Some(pane)) <= 1 {
            let at = self.last_at;
            let new_id = self.add_tab(TabKind::Passage { at }, window, pane.other());
            self.last_passage = new_id;
            self.last_at = at;
            return SplitOutcome::Duplicated { id: new_id, at };
        }
        self.set_pane(id, pane.other());
        self.focus(id);
        SplitOutcome::Moved
    }

    pub fn detach(&mut self, id: TabId) -> Option<DetachOutcome> {
        let tab = self.tab(id)?.clone();
        let source = tab.window;
        let window = self.alloc_window();
        self.ensure_window(window);
        if let Some(tab) = self.tab_mut(id) {
            tab.window = window;
            tab.pane = Pane::Left;
        }
        self.repair(source);
        self.focus(id);
        Some(DetachOutcome { window, source })
    }

    pub fn close(&mut self, id: TabId) -> CloseOutcome {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return CloseOutcome::Closed;
        };
        let tab = self.tabs.remove(idx);
        let window = tab.window;
        self.repair(window);

        if self.focused == id {
            let next = self
                .tabs
                .iter()
                .find(|t| t.window == window)
                .or_else(|| self.tabs.first())
                .map(|t| t.id);
            if let Some(next) = next {
                self.focus(next);
            }
        }
        self.repair(window);
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
            let (window, pane) = self.insert_slot();
            let id = self.add_tab(kind, window, pane);
            OpenResult { id, created: true }
        }
    }

    fn add_tab(&mut self, kind: TabKind, window: WindowId, pane: Pane) -> TabId {
        self.ensure_window(window);
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            kind,
            window,
            pane,
        });
        self.focus(id);
        id
    }

    fn alloc_window(&mut self) -> WindowId {
        let n = self.next_window.get();
        self.next_window.set(n + 1);
        WindowId(n)
    }

    fn ensure_window(&mut self, id: WindowId) {
        if self.window(id).is_some() {
            return;
        }
        let focused = self.focused;
        self.windows.push(WindowRec {
            id,
            focused,
            left_sel: None,
            right_sel: None,
        });
    }

    fn insert_slot(&self) -> (WindowId, Pane) {
        self.tab(self.focused)
            .map(|t| (t.window, t.pane))
            .unwrap_or((WindowId::MAIN, Pane::Left))
    }

    fn focused_pane(&self, window: WindowId) -> Pane {
        self.window(window)
            .and_then(|rec| self.tab(rec.focused))
            .filter(|t| t.window == window)
            .map(|t| t.pane)
            .unwrap_or(Pane::Left)
    }

    fn set_pane(&mut self, id: TabId, pane: Pane) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        let window = tab.window;
        tab.pane = pane;
        self.repair(window);
        self.focus(id);
    }

    fn count(&self, window: WindowId, pane: Option<Pane>) -> usize {
        self.tabs
            .iter()
            .filter(|t| t.window == window && pane.is_none_or(|p| t.pane == p))
            .count()
    }

    fn window(&self, id: WindowId) -> Option<&WindowRec> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn window_mut(&mut self, id: WindowId) -> Option<&mut WindowRec> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn first_in(&self, window: WindowId, pane: Pane) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| t.window == window && t.pane == pane)
            .map(|t| t.id)
    }

    fn sel_ok(&self, id: Option<TabId>, window: WindowId, pane: Pane) -> bool {
        id.is_some_and(|id| {
            self.tab(id)
                .is_some_and(|t| t.window == window && t.pane == pane)
        })
    }

    fn repair(&mut self, window: WindowId) {
        let left = self.first_in(window, Pane::Left);
        let right = self.first_in(window, Pane::Right);
        let left_ok = self
            .window(window)
            .is_some_and(|rec| self.sel_ok(rec.left_sel, window, Pane::Left));
        let right_ok = self
            .window(window)
            .is_some_and(|rec| self.sel_ok(rec.right_sel, window, Pane::Right));
        let focused_here = self
            .window(window)
            .is_some_and(|rec| self.tab(rec.focused).is_some_and(|t| t.window == window));
        let Some(rec) = self.window_mut(window) else {
            return;
        };
        if !left_ok {
            rec.left_sel = left;
        }
        if !right_ok {
            rec.right_sel = right;
        }
        if !focused_here {
            rec.focused = rec.left_sel.or(rec.right_sel).unwrap_or(rec.focused);
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
    fn closing_the_last_tab_leaves_none() {
        let mut ws = start();
        ws.navigate_passage(ws.focused(), r(43, 3, 16));
        let id = ws.focused();
        assert_eq!(ws.close(id), CloseOutcome::Closed);
        assert!(ws.tabs().is_empty());
        assert!(ws.focused_passage_id().is_none());
        assert_eq!(ws.last_at(), r(43, 3, 16));
    }

    #[test]
    fn closing_the_passage_keeps_a_study_tab() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert_eq!(ws.close(passage), CloseOutcome::Closed);
        assert_eq!(ws.tabs().len(), 1);
        assert_eq!(ws.focused(), mhc);
        assert!(ws.tab(mhc).unwrap().kind.is_mhc());
    }

    #[test]
    fn splitting_the_only_passage_duplicates_it() {
        let mut ws = start();
        let left = ws.focused();
        match ws.split_tab(left) {
            SplitOutcome::Duplicated { id, at } => {
                assert_eq!(at, r(1, 1, 1));
                assert!(ws.is_split());
                assert_eq!(ws.tab(left).unwrap().pane, Pane::Left);
                assert_eq!(ws.tab(id).unwrap().pane, Pane::Right);
                assert_eq!(ws.tab(id).unwrap().kind.at(), Some(r(1, 1, 1)));
                assert_eq!(ws.tab(left).unwrap().window, WindowId::MAIN);
            }
            other => panic!("expected a duplicate passage, got {other:?}"),
        }
    }

    #[test]
    fn splitting_matthew_henry_moves_it_beside_the_passage() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(!ws.is_split());
        assert_eq!(ws.split_tab(mhc), SplitOutcome::Moved);
        assert_eq!(ws.tab(passage).unwrap().pane, Pane::Left);
        assert_eq!(ws.tab(mhc).unwrap().pane, Pane::Right);
        assert!(ws.tab(mhc).unwrap().kind.is_mhc());
    }

    #[test]
    fn splitting_a_tab_already_alone_on_its_side_does_nothing() {
        let mut ws = start();
        let left = ws.focused();
        let right = ws.open_passage_beside(left, r(43, 3, 16)).id;
        assert_eq!(ws.split_tab(left), SplitOutcome::AlreadyBeside);
        assert_eq!(ws.split_tab(right), SplitOutcome::AlreadyBeside);
        assert!(!ws.can_split(left));
        assert!(!ws.can_split(right));
        assert_eq!(ws.tabs().len(), 2);
    }

    #[test]
    fn detaching_the_only_passage_leaves_the_window_empty() {
        let mut ws = start();
        ws.navigate_passage(ws.focused(), r(43, 3, 16));
        let id = ws.focused();
        let outcome = ws.detach(id).expect("detach");
        assert_ne!(outcome.window, WindowId::MAIN);
        assert_eq!(outcome.source, WindowId::MAIN);
        assert_eq!(ws.tab(id).unwrap().window, outcome.window);
        assert!(ws.tabs().iter().all(|tab| tab.window != WindowId::MAIN));
        assert_eq!(ws.tabs().len(), 1);
    }

    #[test]
    fn opening_search_twice_reuses_one_tab() {
        let mut ws = start();
        let first = ws.open_search(WindowId::MAIN);
        assert!(first.created);
        let second = ws.open_search(WindowId::MAIN);
        assert!(!second.created);
        assert_eq!(first.id, second.id);
        assert!(ws.has_search(WindowId::MAIN));
        assert_eq!(ws.tabs().iter().filter(|t| t.kind.is_search()).count(), 1);
        let other = ws.detach(ws.focused_passage_id().unwrap()).unwrap().window;
        let there = ws.open_search(other);
        assert!(there.created);
        assert_ne!(there.id, first.id);
        assert!(ws.has_search(other));
    }
}
