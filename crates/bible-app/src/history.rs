use crate::nav::Ref;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    entries: Vec<Ref>,
    index: usize,
}

impl History {
    pub fn new(start: Ref) -> Self {
        Self {
            entries: vec![start],
            index: 0,
        }
    }

    pub fn current(&self) -> Ref {
        self.entries[self.index]
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Record a jump to `to`. Truncates any forward entries.
    pub fn navigate(&mut self, to: Ref) {
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

    pub fn back(&mut self) -> Option<Ref> {
        if !self.can_back() {
            return None;
        }
        self.index -= 1;
        Some(self.current())
    }

    pub fn forward(&mut self) -> Option<Ref> {
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
}
