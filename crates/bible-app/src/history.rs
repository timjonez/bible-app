#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History<T> {
    entries: Vec<T>,
    index: usize,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(start: T) -> Self {
        Self {
            entries: vec![start],
            index: 0,
        }
    }

    pub fn current(&self) -> T {
        self.entries[self.index].clone()
    }

    /// Replace the trail with a single place. Used when a tab first opens.
    pub fn restart(&mut self, at: T) {
        self.entries.clear();
        self.entries.push(at);
        self.index = 0;
    }

    /// Move the current place without pushing a step. Used while a tab follows the passage.
    pub fn retarget(&mut self, to: T) {
        if self.entries.is_empty() {
            self.restart(to);
            return;
        }
        self.entries[self.index] = to;
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Record a jump to `to`. Truncates any forward entries.
    pub fn navigate(&mut self, to: T) {
        if self.current() == to {
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(to);
        if self.entries.len() > 100 {
            let drop = self.entries.len() - 100;
            self.entries.drain(..drop);
        }
        self.index = self.entries.len() - 1;
    }

    pub fn back(&mut self) -> Option<T> {
        if !self.can_back() {
            return None;
        }
        self.index -= 1;
        Some(self.current())
    }

    pub fn forward(&mut self) -> Option<T> {
        if !self.can_forward() {
            return None;
        }
        self.index += 1;
        Some(self.current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::Ref;

    fn r(book: u8, chapter: u8, verse: u8) -> Ref {
        Ref {
            book,
            chapter,
            verse,
        }
    }

    #[test]
    fn jump_then_back_returns_previous_and_forward_redoes() {
        let start = r(6, 20, 1);
        let mut hist = History::new(start);
        let dest = r(2, 21, 13);
        hist.navigate(dest);
        assert_eq!(hist.current(), dest);
        assert!(hist.can_back());
        assert!(!hist.can_forward());
        assert_eq!(hist.back(), Some(start));
        assert_eq!(hist.current(), start);
        assert!(!hist.can_back());
        assert!(hist.can_forward());
        assert_eq!(hist.forward(), Some(dest));
        assert_eq!(hist.current(), dest);
    }

    #[test]
    fn navigate_after_back_drops_forward_stack() {
        let mut hist = History::new(r(1, 1, 1));
        hist.navigate(r(1, 2, 1));
        hist.navigate(r(1, 3, 1));
        hist.back();
        hist.navigate(r(43, 3, 16));
        assert!(!hist.can_forward());
        assert_eq!(hist.current(), r(43, 3, 16));
        assert_eq!(hist.back(), Some(r(1, 2, 1)));
    }

    #[test]
    fn retarget_keeps_the_previous_step() {
        let mut hist = History::new(r(1, 1, 1));
        hist.navigate(r(1, 2, 1));
        hist.retarget(r(43, 3, 16));
        assert_eq!(hist.current(), r(43, 3, 16));
        assert_eq!(hist.back(), Some(r(1, 1, 1)));
        assert_eq!(hist.forward(), Some(r(43, 3, 16)));
    }

    #[test]
    fn restart_drops_the_trail() {
        let mut hist = History::new(1);
        hist.navigate(2);
        hist.restart(9);
        assert_eq!(hist.current(), 9);
        assert!(!hist.can_back());
        assert!(!hist.can_forward());
    }
}
