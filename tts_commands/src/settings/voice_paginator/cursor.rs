use std::num::Saturating;

use tracing::warn;

#[derive(Debug)]
pub(super) struct PageCursor {
    pages: Box<[String]>,
    index: Saturating<usize>,
}

impl PageCursor {
    pub fn new(pages: Box<[String]>) -> Self {
        Self {
            pages,
            index: Saturating(0),
        }
    }

    pub fn jump_start(&mut self) {
        self.index = Saturating(0);
    }

    pub fn can_rewind(&self) -> bool {
        self.index != Saturating(0)
    }

    pub fn rewind(&mut self) {
        self.index -= 1;
    }

    pub fn current(&self) -> &str {
        if let Some(page) = self.pages.get(self.index.0) {
            page
        } else {
            warn!("Ran off the end of the pages: {self:?}");
            ""
        }
    }

    pub fn can_advance(&self) -> bool {
        self.index != Saturating(self.pages.len() - 1)
    }

    pub fn advance(&mut self) {
        self.index = (self.index + Saturating(1)).min(Saturating(self.pages.len() - 1));
    }

    pub fn jump_end(&mut self) {
        self.index = Saturating(self.pages.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::voice_paginator::PageCursor;

    fn make_pages() -> PageCursor {
        PageCursor::new(
            ["a", "b", "c", "d", "e", "f"]
                .into_iter()
                .map(String::from)
                .collect(),
        )
    }

    #[test]
    fn rewind_off_start() {
        let mut cursor = make_pages();
        for _ in 0..25 {
            cursor.rewind();
        }

        assert_eq!(cursor.current(), "a");
    }

    #[test]
    fn rewind_after_end() {
        let mut cursor = make_pages();
        assert!(!cursor.can_rewind());
        assert_eq!(cursor.current(), "a");
        for _ in 0..2 {
            cursor.jump_start();
            assert!(!cursor.can_rewind());
            assert_eq!(cursor.current(), "a");
        }
        for _ in 0..2 {
            cursor.rewind();
            assert!(!cursor.can_rewind());
            assert_eq!(cursor.current(), "a");
        }
    }

    #[test]
    fn advance_past_start() {
        let mut cursor = make_pages();
        for _ in 0..25 {
            cursor.advance();
        }

        assert_eq!(cursor.current(), "f");
    }

    #[test]
    fn advance_after_end() {
        let mut cursor = make_pages();

        assert!(cursor.can_advance());
        assert_eq!(cursor.current(), "a");
        for _ in 0..2 {
            cursor.jump_end();
            assert!(!cursor.can_advance());
            assert_eq!(cursor.current(), "f");
        }
        for _ in 0..2 {
            cursor.advance();
            assert!(!cursor.can_advance());
            assert_eq!(cursor.current(), "f");
        }
    }
}
