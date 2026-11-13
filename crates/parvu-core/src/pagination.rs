//! Page arithmetic (port of `core/pagination.py`). Pages are 1-indexed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paginator {
    total_rows: u64,
    page_size: u64,
    total_pages: u64,
}

impl Paginator {
    /// `page_size` of 0 is treated as 1 so arithmetic never divides by zero.
    pub fn new(total_rows: u64, page_size: u64) -> Self {
        let page_size = page_size.max(1);
        Self { total_rows, page_size, total_pages: Self::pages_for(total_rows, page_size) }
    }

    fn pages_for(total_rows: u64, page_size: u64) -> u64 {
        total_rows.div_ceil(page_size).max(1)
    }

    pub fn total_rows(&self) -> u64 {
        self.total_rows
    }
    pub fn total_pages(&self) -> u64 {
        self.total_pages
    }
    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    pub fn clamp_page(&self, page: u64) -> u64 {
        page.clamp(1, self.total_pages)
    }

    /// Row offset of the first row on `page` (after clamping).
    pub fn page_offset(&self, page: u64) -> u64 {
        (self.clamp_page(page) - 1) * self.page_size
    }

    pub fn recalculate(&mut self, total_rows: u64) {
        self.total_rows = total_rows;
        self.total_pages = Self::pages_for(total_rows, self.page_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let p = Paginator::new(250, 100);
        assert_eq!(p.total_pages(), 3);
        assert_eq!(p.page_offset(1), 0);
        assert_eq!(p.page_offset(3), 200);
        assert_eq!(p.clamp_page(0), 1);
        assert_eq!(p.clamp_page(99), 3);
    }

    #[test]
    fn empty_has_one_page() {
        let p = Paginator::new(0, 100);
        assert_eq!(p.total_pages(), 1);
        assert_eq!(p.page_offset(5), 0);
    }

    #[test]
    fn exact_multiple_and_recalc() {
        let mut p = Paginator::new(200, 100);
        assert_eq!(p.total_pages(), 2);
        p.recalculate(201);
        assert_eq!(p.total_pages(), 3);
    }

    #[test]
    fn zero_page_size_is_safe() {
        let p = Paginator::new(5, 0);
        assert_eq!(p.page_size(), 1);
        assert_eq!(p.total_pages(), 5);
    }
}
