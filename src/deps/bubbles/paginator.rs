//! Pagination state and its dot/arabic renderings.

/// How pagination is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Type {
    /// `1/4`.
    #[default]
    Arabic,
    /// `•○○○`.
    Dots,
}

/// The paginator model.
///
/// Glow builds these as zero values with only `Type` set, so the defaults here
/// are Go's zero values, not `paginator.New`'s.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// How to render.
    pub kind: Type,
    /// Current page, zero-based.
    pub page: usize,
    /// Items per page.
    pub per_page: usize,
    /// Number of pages.
    pub total_pages: usize,
    /// Marker for the current page in dot mode.
    pub active_dot: String,
    /// Marker for other pages in dot mode.
    pub inactive_dot: String,
    /// Format for arabic mode.
    pub arabic_format: String,
}

impl Model {
    /// A paginator rendered as dots.
    pub fn dots() -> Model {
        Model {
            kind: Type::Dots,
            ..Model::default()
        }
    }

    /// Recomputes `total_pages` from an item count, and returns it.
    pub fn set_total_pages(&mut self, items: usize) -> usize {
        if items < 1 || self.per_page == 0 {
            return self.total_pages;
        }
        let mut n = items / self.per_page;
        if items % self.per_page > 0 {
            n += 1;
        }
        self.total_pages = n;
        n
    }

    /// How many items the current page holds.
    pub fn items_on_page(&self, total_items: usize) -> usize {
        if total_items < 1 {
            return 0;
        }
        let (start, end) = self.slice_bounds(total_items);
        end.saturating_sub(start)
    }

    /// The half-open slice range the current page covers.
    pub fn slice_bounds(&self, length: usize) -> (usize, usize) {
        let start = self.page * self.per_page;
        let end = (self.page * self.per_page + self.per_page).min(length);
        (start, end)
    }

    /// Moves back one page, stopping at the first.
    pub fn prev_page(&mut self) {
        if self.page > 0 {
            self.page -= 1;
        }
    }

    /// Moves forward one page, stopping at the last.
    pub fn next_page(&mut self) {
        if !self.on_last_page() {
            self.page += 1;
        }
    }

    /// Whether the current page is the last one.
    pub fn on_last_page(&self) -> bool {
        self.page + 1 == self.total_pages
    }

    /// Renders the pagination.
    pub fn view(&self) -> String {
        match self.kind {
            Type::Dots => (0..self.total_pages)
                .map(|i| {
                    if i == self.page {
                        self.active_dot.as_str()
                    } else {
                        self.inactive_dot.as_str()
                    }
                })
                .collect(),
            Type::Arabic => {
                let format = if self.arabic_format.is_empty() {
                    "%d/%d"
                } else {
                    &self.arabic_format
                };
                format
                    .replacen("%d", &(self.page + 1).to_string(), 1)
                    .replacen("%d", &self.total_pages.to_string(), 1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_pages_rounds_up() {
        let mut p = Model {
            per_page: 3,
            ..Model::default()
        };
        assert_eq!(p.set_total_pages(9), 3);
        assert_eq!(p.set_total_pages(10), 4);
        assert_eq!(p.set_total_pages(0), 4, "an empty set leaves it alone");
    }

    #[test]
    fn slice_bounds_stop_at_the_end() {
        let p = Model {
            per_page: 3,
            page: 1,
            total_pages: 2,
            ..Model::default()
        };
        assert_eq!(p.slice_bounds(5), (3, 5));
        assert_eq!(p.items_on_page(5), 2);
        assert_eq!(p.items_on_page(0), 0);
    }

    #[test]
    fn paging_stops_at_the_ends() {
        let mut p = Model {
            per_page: 1,
            total_pages: 2,
            ..Model::default()
        };
        p.prev_page();
        assert_eq!(p.page, 0);
        p.next_page();
        assert_eq!(p.page, 1);
        p.next_page();
        assert_eq!(p.page, 1);
        assert!(p.on_last_page());
    }

    #[test]
    fn dots_mark_the_current_page() {
        let mut p = Model::dots();
        p.active_dot = "•".into();
        p.inactive_dot = "○".into();
        p.total_pages = 3;
        p.page = 1;
        assert_eq!(p.view(), "○•○");
    }

    #[test]
    fn arabic_counts_from_one() {
        let p = Model {
            kind: Type::Arabic,
            page: 1,
            total_pages: 4,
            ..Model::default()
        };
        assert_eq!(p.view(), "2/4");
    }
}
