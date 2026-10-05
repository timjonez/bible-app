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
    Bookmarks,
    Notes,
    Search,
    /// An empty tab with ways to open a passage or a study view.
    Blank,
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

    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank)
    }

    /// Search, Henry, Treasury, or library — views that share a split's right pane.
    pub fn is_study_view(&self) -> bool {
        matches!(
            self,
            Self::Mhc { .. } | Self::Tsk { .. } | Self::Library { .. } | Self::Search
        )
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
    /// When set, this view is the right-hand side of that tab and has no tab of its own.
    pub host: Option<TabId>,
    /// Guest currently shown on the right of this tab.
    pub visible_guest: Option<TabId>,
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
    /// A new passage was opened on the right of this tab.
    Duplicated { id: TabId, at: Ref },
    /// This view moved to the right of another tab.
    Moved,
    /// This window already has a split.
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
    /// The tab shown in the bar. A split's other view is not a tab.
    selected: Option<TabId>,
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
                host: None,
                visible_guest: None,
            }],
            windows: vec![WindowRec {
                id: WindowId::MAIN,
                focused: id,
                selected: Some(id),
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
            .any(|t| t.window == window && t.host.is_some())
    }

    /// Guests drawn in `host`'s right-hand stack, in open order.
    pub fn guests_of(&self, host: TabId) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|t| t.host == Some(host))
            .map(|t| t.id)
            .collect()
    }

    /// The view shown inside `host`, when that tab is split.
    pub fn guest_of(&self, host: TabId) -> Option<TabId> {
        let visible = self.tab(host).and_then(|t| t.visible_guest);
        if visible.is_some_and(|id| self.tab(id).is_some_and(|t| t.host == Some(host))) {
            return visible;
        }
        self.guests_of(host).first().copied()
    }

    /// Show `guest` in `host`'s right-hand stack.
    pub fn show_guest(&mut self, host: TabId, guest: TabId) -> bool {
        if !self.tab(guest).is_some_and(|t| t.host == Some(host)) {
            return false;
        }
        self.focus(guest);
        true
    }

    /// Passage host whose right-hand guests are study views, when this window is split that way.
    pub fn study_stack_host(&self, window: WindowId) -> Option<TabId> {
        let host = self.tabs.iter().find(|t| {
            t.window == window && t.host.is_none() && self.tabs.iter().any(|g| g.host == Some(t.id))
        })?;
        if !host.kind.is_passage() {
            return None;
        }
        let guests = self.guests_of(host.id);
        if guests.is_empty() {
            return None;
        }
        guests
            .iter()
            .all(|&id| self.tab(id).is_some_and(|t| t.kind.is_study_view()))
            .then_some(host.id)
    }

    pub fn stack_guest_kind(&self, host: TabId, pred: impl Fn(&TabKind) -> bool) -> Option<TabId> {
        self.guests_of(host)
            .into_iter()
            .find(|&id| self.tab(id).is_some_and(|t| pred(&t.kind)))
    }

    /// Put `id` in this window's study stack, or start a split with a passage.
    /// False when the window's split is a Bible|Bible (or other non-study) pair.
    pub fn place_in_study_stack(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tab(id).cloned() else {
            return false;
        };
        if let Some(host) = tab.host {
            return self.show_guest(host, id);
        }
        if tab.kind.is_study_view() {
            if let Some(host) = self.study_stack_host(tab.window) {
                if host == id {
                    return false;
                }
                self.embed(id, host);
                return true;
            }
        }
        self.move_beside(id)
    }

    /// The passage on the other side of `id`'s split, when that side is a passage.
    pub fn passage_beside(&self, id: TabId) -> Option<TabId> {
        let tab = self.tab(id)?;
        if let Some(host) = tab.host {
            return self
                .tab(host)
                .filter(|partner| partner.kind.is_passage())
                .map(|partner| partner.id);
        }
        self.guests_of(id).into_iter().find(|&partner| {
            self.tab(partner)
                .is_some_and(|partner| partner.kind.is_passage())
        })
    }

    /// The library on the other side of `id`'s split, when that side is a library.
    pub fn library_beside(&self, id: TabId) -> Option<TabId> {
        let tab = self.tab(id)?;
        if let Some(host) = tab.host {
            return self
                .tab(host)
                .filter(|partner| matches!(partner.kind, TabKind::Library { .. }))
                .map(|partner| partner.id);
        }
        self.guests_of(id).into_iter().find(|&partner| {
            self.tab(partner)
                .is_some_and(|partner| matches!(partner.kind, TabKind::Library { .. }))
        })
    }

    fn paired_with_passage(&self, id: TabId) -> bool {
        let Some(host) = self.tab(id).and_then(|t| t.host) else {
            return false;
        };
        self.tab(host).is_some_and(|t| t.kind.is_passage())
    }

    fn follows_in_split(&self, id: TabId) -> bool {
        self.tab(id)
            .is_some_and(|t| t.kind.follows_verse() && self.paired_with_passage(id))
    }

    /// False once this window already has a split. The split belongs to one tab.
    pub fn can_split(&self, id: TabId) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        tab.host.is_none() && !self.is_window_split(tab.window)
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
        let selected = rec.selected.filter(|id| {
            self.tab(*id)
                .is_some_and(|t| t.window == window && t.host.is_none())
        });
        if let Some(id) = selected {
            ids.push(id);
            if let Some(guest) = self.guest_of(id) {
                ids.push(guest);
            }
        }
        if ids.is_empty() {
            if let Some(t) = self
                .tabs
                .iter()
                .find(|t| t.window == window && t.host.is_none())
            {
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_search(&self, window: WindowId) -> bool {
        self.find_search(window).is_some()
    }

    pub fn find_search(&self, window: WindowId) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| t.window == window && t.kind.is_search())
            .map(|t| t.id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn find_kind(&self, pred: impl Fn(&TabKind) -> bool) -> Option<TabId> {
        self.tabs.iter().find(|t| pred(&t.kind)).map(|t| t.id)
    }

    pub fn focus(&mut self, id: TabId) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let window = tab.window;
        let host = tab.host;
        let selected = host.unwrap_or(id);
        let is_passage = tab.kind.is_passage();
        let at = tab.kind.at();
        self.focused = id;
        if is_passage {
            self.last_passage = id;
            if let Some(at) = at {
                self.last_at = at;
            }
        }
        if let Some(host) = host {
            if let Some(tab) = self.tab_mut(host) {
                tab.visible_guest = Some(id);
            }
        }
        if let Some(rec) = self.window_mut(window) {
            rec.focused = id;
            rec.selected = Some(selected);
        }
    }

    /// Record where a tab landed after a tab view moved it.
    /// A dropped page is a real tab. Its split, if it had one, comes with it.
    pub fn place(&mut self, id: TabId, window: WindowId) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let source = tab.window;
        let guests = self.guests_of(id);
        if source == window
            && tab.host.is_none()
            && guests
                .iter()
                .all(|&g| self.tab(g).is_some_and(|t| t.window == window))
        {
            self.focus(id);
            return;
        }
        self.ensure_window(window);
        let dest_taken = self.is_window_split(window)
            && self
                .tabs
                .iter()
                .any(|t| t.window == window && t.host.is_some() && t.host != Some(id));
        if let Some(tab) = self.tab_mut(id) {
            tab.window = window;
            tab.host = None;
            if dest_taken {
                tab.visible_guest = None;
            }
        }
        for guest in guests {
            if dest_taken {
                self.release_guest(guest);
            }
            if let Some(tab) = self.tab_mut(guest) {
                tab.window = window;
            }
        }
        self.repair(source);
        self.repair(window);
        self.focus(id);
    }

    pub fn navigate_passage(&mut self, id: TabId, at: Ref) {
        if self.set_passage_at(id, at) {
            self.follow_verse(at);
        }
    }

    /// Move a passage, and study tabs that follow it, without moving `keep`.
    pub fn navigate_passage_except(&mut self, id: TabId, at: Ref, keep: TabId) {
        if !self.set_passage_at(id, at) {
            return;
        }
        let following: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| tab.id != keep && self.follows_in_split(tab.id))
            .map(|tab| tab.id)
            .collect();
        for id in following {
            self.set_study_at(id, at);
        }
    }

    fn set_passage_at(&mut self, id: TabId, at: Ref) -> bool {
        let Some(tab) = self.tab_mut(id) else {
            return false;
        };
        let TabKind::Passage { at: slot } = &mut tab.kind else {
            return false;
        };
        *slot = at;
        self.last_passage = id;
        self.last_at = at;
        true
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

    /// Update MHC/TSK guests that follow a paired chapter.
    pub fn follow_verse(&mut self, at: Ref) {
        let following: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| self.follows_in_split(tab.id))
            .map(|tab| tab.id)
            .collect();
        for id in following {
            self.set_study_at(id, at);
        }
    }

    pub fn focused_in(&self, window: WindowId) -> Option<TabId> {
        let id = self.window(window)?.focused;
        self.tab(id).filter(|t| t.window == window).map(|t| t.id)
    }

    /// Move a Matthew Henry or Treasury tab without changing whether it follows.
    pub fn set_study_at(&mut self, id: TabId, at: Ref) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        match &mut tab.kind {
            TabKind::Mhc { at: slot, .. } | TabKind::Tsk { at: slot, .. } => *slot = at,
            _ => {}
        }
    }

    pub fn set_library_headword(&mut self, id: TabId, headword: Option<String>) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        if let TabKind::Library { headword: slot, .. } = &mut tab.kind {
            *slot = headword;
        }
    }

    pub fn set_follow(&mut self, id: TabId, follow: bool) {
        let last_at = self.last_at;
        let follow = follow && self.paired_with_passage(id);
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
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Passage { at }, window);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    pub fn open_passage_in(&mut self, window: WindowId, at: Ref) -> OpenResult {
        let id = self.add_tab(TabKind::Passage { at }, window);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    /// Open `at` on the right of `from`'s tab.
    /// A window that already has a split gets a normal tab instead.
    pub fn open_passage_beside(&mut self, from: TabId, at: Ref) -> OpenResult {
        let Some(from_tab) = self.tab(from).cloned() else {
            return self.open_passage(at);
        };
        let host = from_tab.host.unwrap_or(from);
        let window = self
            .tab(host)
            .map(|tab| tab.window)
            .unwrap_or(from_tab.window);
        if self.is_window_split(window) {
            return self.open_passage_in(window, at);
        }
        let id = self.add_tab(TabKind::Passage { at }, window);
        self.embed(id, host);
        self.last_passage = id;
        self.last_at = at;
        OpenResult { id, created: true }
    }

    pub fn open_mhc(&mut self, at: Ref) -> OpenResult {
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Mhc { at, follow: false }, window);
        OpenResult { id, created: true }
    }

    pub fn open_tsk(&mut self, at: Ref) -> OpenResult {
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Tsk { at, follow: false }, window);
        OpenResult { id, created: true }
    }

    pub fn open_library(&mut self, module: String, headword: Option<String>) -> OpenResult {
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Library { module, headword }, window);
        OpenResult { id, created: true }
    }

    pub fn open_bookmarks(&mut self) -> OpenResult {
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Bookmarks, window);
        OpenResult { id, created: true }
    }

    pub fn open_notes(&mut self) -> OpenResult {
        let window = self.focused_window();
        let id = self.add_tab(TabKind::Notes, window);
        OpenResult { id, created: true }
    }

    pub fn open_blank(&mut self, window: WindowId) -> OpenResult {
        let id = self.add_tab(TabKind::Blank, window);
        OpenResult { id, created: true }
    }

    pub fn open_search(&mut self, window: WindowId) -> OpenResult {
        let id = self.add_tab(TabKind::Search, window);
        OpenResult { id, created: true }
    }

    /// Draw `id` on the right of another tab in its window.
    /// False when that would be a second split, or when `id` is the only tab.
    pub fn move_beside(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tab(id).cloned() else {
            return false;
        };
        if tab.host.is_some() || self.is_window_split(tab.window) {
            return false;
        }
        let Some(dest) = self.embed_target(tab.window, id) else {
            return false;
        };
        self.embed(id, dest);
        true
    }

    pub fn split_tab(&mut self, id: TabId) -> SplitOutcome {
        let Some(tab) = self.tab(id).cloned() else {
            return SplitOutcome::AlreadyBeside;
        };
        if tab.host.is_some() || self.is_window_split(tab.window) {
            return SplitOutcome::AlreadyBeside;
        }
        let window = tab.window;
        if tab.kind.is_passage() {
            let at = tab.kind.at().unwrap_or(self.last_at);
            let new_id = self.add_tab(TabKind::Passage { at }, window);
            self.embed(new_id, id);
            self.last_passage = new_id;
            self.last_at = at;
            return SplitOutcome::Duplicated { id: new_id, at };
        }
        if let Some(dest) = self.embed_target(window, id) {
            self.embed(id, dest);
            return SplitOutcome::Moved;
        }
        let at = self.last_at;
        let new_id = self.add_tab(TabKind::Passage { at }, window);
        self.embed(new_id, id);
        self.last_passage = new_id;
        self.last_at = at;
        SplitOutcome::Duplicated { id: new_id, at }
    }

    pub fn detach(&mut self, id: TabId) -> Option<DetachOutcome> {
        let tab = self.tab(id)?.clone();
        let source = tab.window;
        let window = self.alloc_window();
        self.ensure_window(window);
        let guests = if tab.host.is_none() {
            self.guests_of(id)
        } else {
            Vec::new()
        };
        if tab.host.is_some() {
            self.release_guest(id);
        }
        if let Some(tab) = self.tab_mut(id) {
            tab.window = window;
            tab.host = None;
        }
        for guest in guests {
            if let Some(tab) = self.tab_mut(guest) {
                tab.window = window;
            }
        }
        self.repair(source);
        self.repair(window);
        self.focus(id);
        Some(DetachOutcome { window, source })
    }

    pub fn close(&mut self, id: TabId) -> CloseOutcome {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return CloseOutcome::Closed;
        };
        let tab = self.tabs.remove(idx);
        let window = tab.window;
        let guests: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|t| t.host == Some(tab.id))
            .map(|t| t.id)
            .collect();
        for guest in guests {
            self.release_guest(guest);
        }
        if let Some(host) = tab.host {
            let next = self.guests_of(host).first().copied();
            if let Some(tab) = self.tab_mut(host) {
                if tab.visible_guest == Some(id) {
                    tab.visible_guest = next;
                }
            }
        }
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

    fn add_tab(&mut self, kind: TabKind, window: WindowId) -> TabId {
        self.ensure_window(window);
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            kind,
            window,
            host: None,
            visible_guest: None,
        });
        self.focus(id);
        id
    }

    /// `id` becomes a right-hand view of `host` and drops out of the tab bar.
    fn embed(&mut self, id: TabId, host: TabId) {
        if let Some(tab) = self.tab_mut(id) {
            tab.host = Some(host);
        }
        if let Some(tab) = self.tab_mut(host) {
            tab.visible_guest = Some(id);
        }
        if self.paired_with_passage(id)
            && self
                .tab(id)
                .is_some_and(|t| matches!(t.kind, TabKind::Mhc { .. } | TabKind::Tsk { .. }))
        {
            self.set_follow(id, true);
        }
        self.focus(id);
    }

    /// Drop `id` out of a split. Follow turns off; the view becomes a real tab.
    fn release_guest(&mut self, id: TabId) {
        let host = self.tab(id).and_then(|t| t.host);
        self.set_follow(id, false);
        if let Some(tab) = self.tab_mut(id) {
            tab.host = None;
        }
        if let Some(host) = host {
            let next = self.guests_of(host).first().copied();
            if let Some(tab) = self.tab_mut(host) {
                if tab.visible_guest == Some(id) {
                    tab.visible_guest = next;
                }
            }
        }
    }

    /// A passage tab when this window has one, otherwise any other real tab.
    fn embed_target(&self, window: WindowId, id: TabId) -> Option<TabId> {
        let mut other = None;
        for tab in self
            .tabs
            .iter()
            .filter(|t| t.window == window && t.host.is_none() && t.id != id)
        {
            if tab.kind.is_passage() {
                return Some(tab.id);
            }
            if other.is_none() {
                other = Some(tab.id);
            }
        }
        other
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
            selected: None,
        });
    }

    fn focused_window(&self) -> WindowId {
        self.tab(self.focused)
            .map(|tab| tab.window)
            .unwrap_or(WindowId::MAIN)
    }

    fn window(&self, id: WindowId) -> Option<&WindowRec> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn window_mut(&mut self, id: WindowId) -> Option<&mut WindowRec> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn repair(&mut self, window: WindowId) {
        let real = self
            .tabs
            .iter()
            .find(|t| t.window == window && t.host.is_none())
            .map(|t| t.id);
        let selected_ok = self
            .window(window)
            .and_then(|rec| rec.selected)
            .is_some_and(|id| {
                self.tab(id)
                    .is_some_and(|t| t.window == window && t.host.is_none())
            });
        let focused_ok = self
            .window(window)
            .is_some_and(|rec| self.tab(rec.focused).is_some_and(|t| t.window == window));
        let Some(rec) = self.window_mut(window) else {
            return;
        };
        if !selected_ok {
            rec.selected = real;
        }
        if !focused_ok {
            rec.focused = rec.selected.unwrap_or(rec.focused);
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
    fn new_tab_is_blank_and_not_unique() {
        let mut ws = start();
        let passage = ws.focused();
        let first = ws.open_blank(WindowId::MAIN);
        let second = ws.open_blank(WindowId::MAIN);
        assert!(first.created && second.created);
        assert_ne!(first.id, second.id);
        assert!(ws.tab(first.id).unwrap().kind.is_blank());
        assert!(ws.tab(first.id).unwrap().host.is_none());
        assert_eq!(ws.focused(), second.id);
        assert_eq!(ws.focused_passage_id(), Some(passage));
        assert_eq!(ws.tabs().len(), 3);
    }

    #[test]
    fn new_tab_on_an_empty_window() {
        let mut ws = start();
        let id = ws.focused();
        assert_eq!(ws.close(id), CloseOutcome::Closed);
        let blank = ws.open_blank(WindowId::MAIN);
        assert_eq!(ws.tabs().len(), 1);
        assert!(ws.tab(blank.id).unwrap().kind.is_blank());
        assert!(ws.focused_passage_id().is_none());
        assert_eq!(ws.focused(), blank.id);
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
    fn open_mhc_creates_another_tab() {
        let mut ws = start();
        let a = ws.open_mhc(r(1, 1, 1));
        assert!(a.created);
        let b = ws.open_mhc(r(1, 2, 4));
        assert!(b.created);
        assert_ne!(a.id, b.id);
        assert_eq!(ws.tabs().len(), 3);
        assert_eq!(ws.tab(a.id).and_then(|t| t.kind.at()), Some(r(1, 1, 1)));
        assert_eq!(ws.tab(b.id).and_then(|t| t.kind.at()), Some(r(1, 2, 4)));
        assert_eq!(ws.tabs().iter().filter(|t| t.kind.is_mhc()).count(), 2);
        let c = ws.open_tsk(r(1, 1, 1));
        assert!(c.created);
        let d = ws.open_tsk(r(43, 1, 1));
        assert!(d.created);
        assert_ne!(c.id, d.id);
        assert_eq!(ws.tabs().iter().filter(|t| t.kind.is_tsk()).count(), 2);
    }

    #[test]
    fn library_and_marks_each_open_a_tab() {
        let mut ws = start();
        let lib = ws.open_library("Easton".into(), Some("God".into()));
        let lib2 = ws.open_library("Webster".into(), Some("Divide".into()));
        assert_ne!(lib.id, lib2.id);
        match &ws.tab(lib.id).unwrap().kind {
            TabKind::Library { module, headword } => {
                assert_eq!(module, "Easton");
                assert_eq!(headword.as_deref(), Some("God"));
            }
            other => panic!("expected library, got {other:?}"),
        }
        match &ws.tab(lib2.id).unwrap().kind {
            TabKind::Library { module, headword } => {
                assert_eq!(module, "Webster");
                assert_eq!(headword.as_deref(), Some("Divide"));
            }
            other => panic!("expected library, got {other:?}"),
        }
        let marks = ws.open_bookmarks();
        let notes = ws.open_notes();
        let marks2 = ws.open_bookmarks();
        assert_ne!(marks.id, notes.id);
        assert_ne!(marks.id, marks2.id);
        assert!(matches!(ws.tab(marks.id).unwrap().kind, TabKind::Bookmarks));
        assert!(matches!(ws.tab(notes.id).unwrap().kind, TabKind::Notes));
        assert!(matches!(
            ws.tab(marks2.id).unwrap().kind,
            TabKind::Bookmarks
        ));
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
        assert!(ws.tab(left).unwrap().host.is_none());
        assert_eq!(ws.tab(right.id).unwrap().host, Some(left));
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
        assert!(ws.place_in_study_stack(mhc));
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(tsk));
        let lib = ws.open_library("Easton".into(), Some("God".into())).id;
        assert!(ws.place_in_study_stack(lib));
        let notes = ws.open_notes().id;
        ws.navigate_passage(passage, r(43, 3, 16));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(43, 3, 16)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(43, 3, 16)));
        match &ws.tab(lib).unwrap().kind {
            TabKind::Library { headword, .. } => assert_eq!(headword.as_deref(), Some("God")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(ws.tab(notes).unwrap().kind, TabKind::Notes));
        assert!(ws.tab(notes).unwrap().host.is_none());
        ws.set_follow(mhc, false);
        ws.navigate_passage(passage, r(1, 2, 3));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(43, 3, 16)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(1, 2, 3)));
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(ws.tab(tsk).unwrap().kind.follows_verse());
    }

    #[test]
    fn set_study_at_keeps_follow() {
        let mut ws = start();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(mhc));
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(tsk));
        ws.set_follow(mhc, false);
        ws.set_study_at(mhc, r(19, 23, 1));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(19, 23, 1)));
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(1, 1, 1)));
        assert!(ws.tab(tsk).unwrap().kind.follows_verse());
    }

    #[test]
    fn opening_mhc_again_leaves_the_first_tab() {
        let mut ws = start();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        let second = ws.open_mhc(r(19, 23, 1)).id;
        assert_ne!(mhc, second);
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(1, 1, 1)));
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert_eq!(ws.tab(second).unwrap().kind.at(), Some(r(19, 23, 1)));
        assert!(!ws.tab(second).unwrap().kind.follows_verse());
        assert!(ws.tab(mhc).unwrap().host.is_none());
        assert!(ws.tab(second).unwrap().host.is_none());
    }

    #[test]
    fn move_beside_splits_stacked_tabs() {
        let mut ws = start();
        let a = ws.focused();
        let b = ws.open_passage(r(43, 3, 16)).id;
        assert!(!ws.is_split());
        assert!(ws.move_beside(b));
        assert!(ws.is_split());
        assert!(ws.tab(a).unwrap().host.is_none());
        assert_eq!(ws.tab(b).unwrap().host, Some(a));
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
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
        assert!(ws.find_kind(|k| k.is_mhc()).is_none());
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
                assert!(ws.tab(left).unwrap().host.is_none());
                assert_eq!(ws.tab(id).unwrap().host, Some(left));
                assert_eq!(ws.tab(id).unwrap().kind.at(), Some(r(1, 1, 1)));
                assert_eq!(ws.tab(left).unwrap().window, WindowId::MAIN);
                assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
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
        assert!(ws.tab(passage).unwrap().host.is_none());
        assert_eq!(ws.tab(mhc).unwrap().host, Some(passage));
        assert!(ws.tab(mhc).unwrap().kind.is_mhc());
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
        assert_eq!(ws.visible().len(), 2);
    }

    #[test]
    fn splitting_a_tab_already_alone_on_its_side_does_nothing() {
        let mut ws = start();
        let left = ws.focused();
        let right = ws.open_passage_beside(left, r(43, 3, 16)).id;
        assert_eq!(ws.split_tab(left), SplitOutcome::AlreadyBeside);
        assert_eq!(ws.split_tab(right), SplitOutcome::AlreadyBeside);
        assert!(!ws.move_beside(left));
        assert!(!ws.can_split(left));
        assert!(!ws.can_split(right));
        assert_eq!(ws.tabs().len(), 2);
    }

    #[test]
    fn closing_a_split_tab_returns_the_other_view_to_the_bar() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert_eq!(ws.split_tab(mhc), SplitOutcome::Moved);
        assert_eq!(ws.close(passage), CloseOutcome::Closed);
        assert_eq!(ws.tabs().len(), 1);
        assert!(ws.tab(mhc).unwrap().host.is_none());
        assert_eq!(ws.focused(), mhc);
        assert!(!ws.is_split());
    }

    #[test]
    fn a_second_beside_opens_as_its_own_tab() {
        let mut ws = start();
        let left = ws.focused();
        let _right = ws.open_passage_beside(left, r(43, 3, 16)).id;
        let third = ws.open_passage_beside(left, r(19, 23, 1)).id;
        assert!(ws.tab(third).unwrap().host.is_none());
        assert!(ws.tab(third).unwrap().kind.is_passage());
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 2);
        assert!(ws.is_split());
    }

    #[test]
    fn splitting_the_only_study_tab_opens_a_passage_inside_it() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert_eq!(ws.close(passage), CloseOutcome::Closed);
        match ws.split_tab(mhc) {
            SplitOutcome::Duplicated { id, at } => {
                assert_eq!(at, r(1, 1, 1));
                assert!(ws.tab(id).unwrap().kind.is_passage());
                assert_eq!(ws.tab(id).unwrap().host, Some(mhc));
                assert!(ws.tab(mhc).unwrap().host.is_none());
                assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
            }
            other => panic!("expected a passage inside the study tab, got {other:?}"),
        }
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
    fn opening_search_twice_creates_two_tabs() {
        let mut ws = start();
        let first = ws.open_search(WindowId::MAIN);
        assert!(first.created);
        let second = ws.open_search(WindowId::MAIN);
        assert!(second.created);
        assert_ne!(first.id, second.id);
        assert!(ws.has_search(WindowId::MAIN));
        assert_eq!(ws.tabs().iter().filter(|t| t.kind.is_search()).count(), 2);
        let other = ws.detach(ws.focused_passage_id().unwrap()).unwrap().window;
        let there = ws.open_search(other);
        assert!(there.created);
        assert_ne!(there.id, first.id);
        assert!(ws.has_search(other));
    }

    #[test]
    fn passage_beside_finds_the_bible_on_either_side() {
        let mut ws = start();
        let passage = ws.focused();
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(ws.move_beside(tsk));
        assert_eq!(ws.passage_beside(tsk), Some(passage));
        assert_eq!(ws.passage_beside(passage), None);

        let mut ws = start();
        let passage = ws.focused();
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert_eq!(ws.close(passage), CloseOutcome::Closed);
        let guest = ws.open_passage_beside(tsk, r(19, 23, 1)).id;
        assert_eq!(ws.passage_beside(tsk), Some(guest));
        assert_eq!(ws.tab(guest).unwrap().host, Some(tsk));
    }

    #[test]
    fn library_beside_finds_the_library_on_either_side() {
        let mut ws = start();
        let passage = ws.focused();
        let lib = ws.open_library("Strongs".into(), Some("H1".into())).id;
        assert!(ws.move_beside(lib));
        assert_eq!(ws.library_beside(passage), Some(lib));
        assert_eq!(ws.library_beside(lib), None);
        assert_eq!(ws.passage_beside(lib), Some(passage));
    }

    #[test]
    fn navigating_the_split_can_leave_the_treasury_in_place() {
        let mut ws = start();
        let passage = ws.focused();
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(ws.move_beside(tsk));
        assert!(ws.tab(tsk).unwrap().kind.follows_verse());
        ws.navigate_passage_except(passage, r(43, 3, 16), tsk);
        assert_eq!(ws.tab(passage).unwrap().kind.at(), Some(r(43, 3, 16)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(1, 1, 1)));
        ws.navigate_passage(passage, r(19, 23, 1));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(19, 23, 1)));
    }

    #[test]
    fn standalone_mhc_does_not_follow() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(!ws.tab(tsk).unwrap().kind.follows_verse());
        ws.navigate_passage(passage, r(43, 3, 16));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(1, 1, 1)));
        assert_eq!(ws.tab(tsk).unwrap().kind.at(), Some(r(1, 1, 1)));
        ws.set_follow(mhc, true);
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        ws.navigate_passage(passage, r(19, 23, 1));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(1, 1, 1)));
    }

    #[test]
    fn follow_on_when_embedded_beside_a_passage() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(ws.place_in_study_stack(mhc));
        assert_eq!(ws.tab(mhc).unwrap().host, Some(passage));
        assert!(ws.tab(mhc).unwrap().kind.follows_verse());
        ws.navigate_passage(passage, r(43, 3, 16));
        assert_eq!(ws.tab(mhc).unwrap().kind.at(), Some(r(43, 3, 16)));
    }

    #[test]
    fn closing_split_clears_follow() {
        let mut ws = start();
        let passage = ws.focused();
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(mhc));
        assert!(ws.tab(mhc).unwrap().kind.follows_verse());
        assert_eq!(ws.close(passage), CloseOutcome::Closed);
        assert!(ws.tab(mhc).unwrap().host.is_none());
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(!ws.is_split());
    }

    #[test]
    fn multiple_study_guests_on_one_host() {
        let mut ws = start();
        let left = ws.focused();
        let search = ws.open_search(WindowId::MAIN).id;
        assert!(ws.place_in_study_stack(search));
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(mhc));
        let tsk = ws.open_tsk(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(tsk));
        assert_eq!(ws.tab(search).unwrap().host, Some(left));
        assert_eq!(ws.tab(mhc).unwrap().host, Some(left));
        assert_eq!(ws.tab(tsk).unwrap().host, Some(left));
        assert_eq!(ws.guests_of(left), vec![search, mhc, tsk]);
        assert_eq!(ws.guest_of(left), Some(tsk));
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
        assert!(ws.is_split());
        assert!(!ws.can_split(left));
        assert!(ws.tab(mhc).unwrap().kind.follows_verse());
        assert!(ws.tab(tsk).unwrap().kind.follows_verse());
        ws.show_guest(left, search);
        assert_eq!(ws.guest_of(left), Some(search));
        ws.show_guest(left, mhc);
        assert_eq!(ws.guest_of(left), Some(mhc));
    }

    #[test]
    fn search_then_mhc_reuse_the_right_side() {
        let mut ws = start();
        let left = ws.focused();
        let search = ws.open_search(WindowId::MAIN).id;
        assert!(ws.place_in_study_stack(search));
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(ws.place_in_study_stack(mhc));
        assert_eq!(ws.study_stack_host(WindowId::MAIN), Some(left));
        assert_eq!(ws.stack_guest_kind(left, TabKind::is_search), Some(search));
        assert_eq!(ws.stack_guest_kind(left, TabKind::is_mhc), Some(mhc));
        assert_eq!(ws.guest_of(left), Some(mhc));
        assert_eq!(ws.visible(), vec![left, mhc]);
        ws.show_guest(left, search);
        assert_eq!(ws.visible(), vec![left, search]);
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 1);
        ws.close(mhc);
        assert_eq!(ws.tab(search).unwrap().host, Some(left));
        assert!(ws.is_split());
        assert_eq!(ws.guest_of(left), Some(search));
    }

    #[test]
    fn bible_bible_split_does_not_take_mhc() {
        let mut ws = start();
        let left = ws.focused();
        let right = ws.open_passage_beside(left, r(43, 3, 16)).id;
        assert!(ws.tab(right).unwrap().kind.is_passage());
        let mhc = ws.open_mhc(r(1, 1, 1)).id;
        assert!(ws.study_stack_host(WindowId::MAIN).is_none());
        assert!(!ws.place_in_study_stack(mhc));
        assert!(ws.tab(mhc).unwrap().host.is_none());
        assert!(!ws.tab(mhc).unwrap().kind.follows_verse());
        assert_eq!(ws.tabs().iter().filter(|t| t.host.is_none()).count(), 2);
    }
}
