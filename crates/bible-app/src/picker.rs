use bible_app_db::Book;
use gtk::gio::prelude::ListModelExt;
use gtk::prelude::*;
use relm4::gtk;
use std::sync::Once;

pub fn book_index(books: &[Book], id: u8) -> Option<u32> {
    books.iter().position(|b| b.id == id).map(|i| i as u32)
}

pub fn book_id_at(books: &[Book], index: u32) -> Option<u8> {
    books.get(index as usize).map(|b| b.id)
}

pub fn chapter_index(chapter: u8) -> u32 {
    u32::from(chapter.saturating_sub(1))
}

pub fn chapter_from_index(index: u32) -> Option<u8> {
    u8::try_from(index.checked_add(1)?).ok()
}

pub fn prepare(dropdown: &gtk::DropDown, match_mode: gtk::StringFilterMatchMode) {
    let expr = gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    );
    dropdown.set_expression(Some(expr));
    dropdown.set_enable_search(true);
    dropdown.set_search_match_mode(match_mode);
    dropdown.set_valign(gtk::Align::Center);
}

pub fn install_css() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            .passage-title {
              margin: 0;
            }
            dropdown.passage-picker {
              min-width: 10.5em;
              font-weight: 600;
            }
            dropdown.chapter-picker {
              min-width: 4em;
              font-weight: 600;
            }
            dropdown.passage-picker popover contents,
            dropdown.chapter-picker popover contents {
              background-color: var(--popover-bg-color);
              color: var(--popover-fg-color);
              padding: 6px;
              border-radius: 9px;
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

pub fn fill_books(dropdown: &gtk::DropDown, books: &[Book]) {
    let names: Vec<String> = books.iter().map(|b| b.name.clone()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    dropdown.set_model(Some(&gtk::StringList::new(&refs)));
}

pub fn select_book(dropdown: &gtk::DropDown, books: &[Book], book: u8) {
    let Some(idx) = book_index(books, book) else {
        return;
    };
    if dropdown.selected() != idx {
        dropdown.set_selected(idx);
    }
}

pub fn sync_chapters(dropdown: &gtk::DropDown, max_chapter: u8, chapter: u8) {
    let n = u32::from(max_chapter);
    let current = dropdown.model().map(|m| m.n_items()).unwrap_or(0);
    if current != n {
        let labels: Vec<String> = (1..=max_chapter).map(|c| c.to_string()).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        dropdown.set_model(Some(&gtk::StringList::new(&refs)));
        dropdown.set_enable_search(max_chapter > 20);
    }
    if n == 0 {
        return;
    }
    let idx = chapter_index(chapter).min(n - 1);
    if dropdown.selected() != idx {
        dropdown.set_selected(idx);
    }
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

    #[test]
    fn book_index_uses_id_not_position_plus_one() {
        let books = vec![
            book(1, "Genesis"),
            book(2, "Exodus"),
            book(40, "Matthew"),
            book(43, "John"),
        ];
        assert_eq!(book_index(&books, 1), Some(0));
        assert_eq!(book_index(&books, 40), Some(2));
        assert_eq!(book_index(&books, 43), Some(3));
        assert_eq!(book_index(&books, 66), None);
        assert_eq!(book_id_at(&books, 0), Some(1));
        assert_eq!(book_id_at(&books, 2), Some(40));
        assert_eq!(book_id_at(&books, 9), None);
    }

    #[test]
    fn chapter_index_is_zero_based() {
        assert_eq!(chapter_from_index(0), Some(1));
        assert_eq!(chapter_from_index(39), Some(40));
        assert_eq!(chapter_from_index(149), Some(150));
        assert_eq!(chapter_from_index(u32::MAX), None);
        assert_eq!(chapter_index(1), 0);
        assert_eq!(chapter_index(40), 39);
        assert_eq!(chapter_index(0), 0);
    }
}
