use bible_app_db::Book;
use gtk::glib::object::IsA;
use gtk::prelude::*;
use relm4::gtk;
use std::sync::Once;

/// Protestant KJV canon: Genesis–Malachi.
pub const LAST_OT_BOOK: u8 = 39;
/// Protestant KJV canon: Matthew–Revelation.
pub const FIRST_NT_BOOK: u8 = 40;
pub const LAST_NT_BOOK: u8 = 66;

const BOOK_NAME_PREFIX: &str = "book-";
const CHAPTER_NAME_PREFIX: &str = "chapter-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Testament {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookListItem<'a> {
    Header(Testament),
    Book(&'a Book),
}

pub fn testament(book: u8) -> Option<Testament> {
    match book {
        1..=LAST_OT_BOOK => Some(Testament::Old),
        FIRST_NT_BOOK..=LAST_NT_BOOK => Some(Testament::New),
        _ => None,
    }
}

pub fn book_widget_name(id: u8) -> String {
    format!("{BOOK_NAME_PREFIX}{id}")
}

pub fn book_id_from_name(name: &str) -> Option<u8> {
    let id: u8 = name.strip_prefix(BOOK_NAME_PREFIX)?.parse().ok()?;
    testament(id).map(|_| id)
}

pub fn header_widget_name(testament: Testament) -> &'static str {
    match testament {
        Testament::Old => "header-ot",
        Testament::New => "header-nt",
    }
}

pub fn chapter_widget_name(n: u8) -> String {
    format!("{CHAPTER_NAME_PREFIX}{n}")
}

pub fn chapter_from_name(name: &str) -> Option<u8> {
    let n: u8 = name.strip_prefix(CHAPTER_NAME_PREFIX)?.parse().ok()?;
    (n >= 1).then_some(n)
}

/// `1..=max_chapter`. Empty when `max_chapter` is 0.
pub fn chapter_numbers(max_chapter: u8) -> std::ops::RangeInclusive<u8> {
    1..=max_chapter
}

pub fn chapter_grid_count(max_chapter: u8) -> usize {
    chapter_numbers(max_chapter).count()
}

/// OT header, OT books, NT header, NT books. Headers are not books.
pub fn book_list_items(books: &[Book]) -> Vec<BookListItem<'_>> {
    let mut out = Vec::with_capacity(books.len() + 2);
    let mut seen_ot = false;
    let mut seen_nt = false;
    for book in books {
        match testament(book.id) {
            Some(Testament::Old) if !seen_ot => {
                out.push(BookListItem::Header(Testament::Old));
                seen_ot = true;
            }
            Some(Testament::New) if !seen_nt => {
                out.push(BookListItem::Header(Testament::New));
                seen_nt = true;
            }
            _ => {}
        }
        out.push(BookListItem::Book(book));
    }
    out
}

pub fn book_id_from_row(row: &gtk::ListBoxRow) -> Option<u8> {
    book_id_from_name(row.widget_name().as_str())
}

pub fn chapter_from_child(child: &gtk::FlowBoxChild) -> Option<u8> {
    chapter_from_name(child.widget_name().as_str())
}

pub fn install_css() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            flowbox.chapter-grid {
              padding: 4px 6px 8px 6px;
            }
            flowbox.chapter-grid > flowboxchild {
              padding: 1px;
              margin: 0;
              min-width: 28px;
              min-height: 22px;
            }
            flowbox.chapter-grid label {
              font-size: 0.8em;
              padding: 1px 0;
            }
            flowbox.chapter-grid > flowboxchild:selected label {
              font-weight: 700;
            }
            list.navigation-sidebar row.sidebar-header {
              min-height: 0;
            }
            "#,
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

pub fn fill_books(list: &gtk::ListBox, books: &[Book]) {
    list.remove_all();
    for item in book_list_items(books) {
        match item {
            BookListItem::Header(t) => list.append(&header_row(t)),
            BookListItem::Book(book) => list.append(&book_row(book)),
        }
    }
}

pub fn select_book(list: &gtk::ListBox, book: u8) {
    let mut i = 0;
    while let Some(row) = list.row_at_index(i) {
        if book_id_from_row(&row) == Some(book) {
            list.select_row(Some(&row));
            return;
        }
        i += 1;
    }
}

pub fn sync_chapters(grid: &gtk::FlowBox, book: u8, max_chapter: u8, chapter: u8) {
    let key = format!("chapters-{book}");
    if grid.widget_name().as_str() != key || child_count(grid) != chapter_grid_count(max_chapter) {
        fill_chapters(grid, max_chapter);
        grid.set_widget_name(&key);
    }
    select_chapter(grid, chapter);
}

fn fill_chapters(grid: &gtk::FlowBox, max_chapter: u8) {
    grid.remove_all();
    for n in chapter_numbers(max_chapter) {
        grid.append(&chapter_child(n));
    }
}

fn select_chapter(grid: &gtk::FlowBox, chapter: u8) {
    let mut i = 0;
    while let Some(child) = grid.child_at_index(i) {
        if chapter_from_child(&child) == Some(chapter) {
            grid.select_child(&child);
            reveal_child(grid, &child);
            return;
        }
        i += 1;
    }
}

fn child_count(grid: &gtk::FlowBox) -> usize {
    let mut n = 0;
    while grid.child_at_index(n as i32).is_some() {
        n += 1;
    }
    n
}

fn header_row(testament: Testament) -> gtk::ListBoxRow {
    let title = match testament {
        Testament::Old => "Old Testament",
        Testament::New => "New Testament",
    };
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("title");
    label.add_css_class("title-4");
    label.add_css_class("dim-label");
    label.set_margin_start(8);
    label.set_margin_end(8);
    label.set_margin_top(10);
    label.set_margin_bottom(2);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&label));
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_can_focus(false);
    row.add_css_class("sidebar-header");
    row.set_widget_name(header_widget_name(testament));
    row
}

fn book_row(book: &Book) -> gtk::ListBoxRow {
    let label = gtk::Label::new(Some(&book.name));
    label.set_xalign(0.0);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label.set_margin_top(4);
    label.set_margin_bottom(4);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&label));
    row.set_tooltip_text(Some(&book.name));
    row.set_widget_name(&book_widget_name(book.id));
    row
}

fn chapter_child(n: u8) -> gtk::FlowBoxChild {
    let label = gtk::Label::new(Some(&n.to_string()));
    label.set_justify(gtk::Justification::Center);
    let child = gtk::FlowBoxChild::new();
    child.set_child(Some(&label));
    child.set_widget_name(&chapter_widget_name(n));
    let name = format!("Chapter {n}");
    child.set_tooltip_text(Some(&name));
    child.update_property(&[gtk::accessible::Property::Label(&name)]);
    child
}

fn reveal_child(grid: &gtk::FlowBox, child: &gtk::FlowBoxChild) {
    let Some(bounds) = child.compute_bounds(grid) else {
        return;
    };
    let Some(scroll) = enclosing_scrolled_window(grid) else {
        return;
    };
    let adj = scroll.vadjustment();
    let y = f64::from(bounds.y());
    let h = f64::from(bounds.height());
    let val = adj.value();
    let page = adj.page_size();
    if y < val {
        adj.set_value(y);
    } else if y + h > val + page {
        adj.set_value((y + h - page).max(0.0));
    }
}

fn enclosing_scrolled_window(widget: &impl IsA<gtk::Widget>) -> Option<gtk::ScrolledWindow> {
    let mut parent = widget.parent();
    while let Some(p) = parent {
        if let Ok(scroll) = p.clone().downcast::<gtk::ScrolledWindow>() {
            return Some(scroll);
        }
        parent = p.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: u8, name: &str) -> Book {
        Book {
            id,
            abbrev: name.chars().take(3).collect(),
            name: name.into(),
        }
    }

    fn canon_books() -> Vec<Book> {
        (1..=LAST_NT_BOOK)
            .map(|id| book(id, &format!("Book {id}")))
            .collect()
    }

    #[test]
    fn protestant_ot_nt_split() {
        assert_eq!(testament(1), Some(Testament::Old));
        assert_eq!(testament(LAST_OT_BOOK), Some(Testament::Old));
        assert_eq!(testament(FIRST_NT_BOOK), Some(Testament::New));
        assert_eq!(testament(LAST_NT_BOOK), Some(Testament::New));
        assert_eq!(testament(0), None);
        assert_eq!(testament(LAST_NT_BOOK + 1), None);
    }

    #[test]
    fn book_list_inserts_ot_nt_headers() {
        let books = canon_books();
        let items = book_list_items(&books);
        assert_eq!(items.len(), 68);
        assert_eq!(items[0], BookListItem::Header(Testament::Old));
        assert!(matches!(items[1], BookListItem::Book(b) if b.id == 1));
        assert!(matches!(items[39], BookListItem::Book(b) if b.id == LAST_OT_BOOK));
        assert_eq!(items[40], BookListItem::Header(Testament::New));
        assert!(matches!(items[41], BookListItem::Book(b) if b.id == FIRST_NT_BOOK));
        assert!(matches!(items[67], BookListItem::Book(b) if b.id == LAST_NT_BOOK));
    }

    #[test]
    fn selecting_a_book_row_does_not_treat_headers_as_books() {
        let books = canon_books();
        let items = book_list_items(&books);

        assert_eq!(book_id_from_name(header_widget_name(Testament::Old)), None);
        assert_eq!(book_id_from_name(header_widget_name(Testament::New)), None);
        assert_eq!(book_id_from_name("Old Testament"), None);
        assert_eq!(book_id_from_name(""), None);

        let genesis_idx = items
            .iter()
            .position(|i| matches!(i, BookListItem::Book(b) if b.id == 1))
            .unwrap();
        let matthew_idx = items
            .iter()
            .position(|i| matches!(i, BookListItem::Book(b) if b.id == FIRST_NT_BOOK))
            .unwrap();
        // Old SelectBook used row.index() + 1 as the book id.
        assert_ne!(genesis_idx as u8 + 1, 1);
        assert_ne!(matthew_idx as u8 + 1, FIRST_NT_BOOK);
        assert_eq!(book_id_from_name(&book_widget_name(1)), Some(1));
        assert_eq!(
            book_id_from_name(&book_widget_name(FIRST_NT_BOOK)),
            Some(FIRST_NT_BOOK)
        );

        for (idx, item) in items.iter().enumerate() {
            match item {
                BookListItem::Header(t) => {
                    assert_eq!(book_id_from_name(header_widget_name(*t)), None, "idx {idx}");
                }
                BookListItem::Book(b) => {
                    assert_eq!(
                        book_id_from_name(&book_widget_name(b.id)),
                        Some(b.id),
                        "idx {idx}"
                    );
                }
            }
        }
    }

    #[test]
    fn chapter_grid_count_matches_max_chapter() {
        assert_eq!(chapter_grid_count(1), 1);
        assert_eq!(chapter_grid_count(40), 40);
        assert_eq!(chapter_grid_count(150), 150);
        assert_eq!(chapter_grid_count(0), 0);
        assert_eq!(
            chapter_numbers(40).collect::<Vec<_>>(),
            (1..=40).collect::<Vec<_>>()
        );
    }
}
