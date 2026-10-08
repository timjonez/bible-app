//! A blank tab: the passage book and chapter menus, or one of the study views.
//! Picking one fills this tab. Opening a second tab and closing this one
//! makes the tab bar animate the swap.
use crate::picker;
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::Book;
use relm4::{adw, gtk};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    Search,
    Mhc,
    Tsk,
    Library,
    Notes,
    Bookmarks,
}

pub struct BlankPage {
    pub root: gtk::CenterBox,
    pub book: gtk::DropDown,
    pub chapter: gtk::DropDown,
    pub syncing: Rc<Cell<bool>>,
}

pub fn build(
    id: TabId,
    sender: relm4::Sender<crate::app::Msg>,
    library: bool,
    marks: bool,
    books: &[Book],
) -> BlankPage {
    let syncing = Rc::new(Cell::new(false));

    let book = gtk::DropDown::from_strings(&[]);
    picker::prepare(&book, gtk::StringFilterMatchMode::Substring);
    picker::fill_books(&book, books);
    book.add_css_class("passage-picker");
    book.set_tooltip_text(Some("Book"));
    book.update_property(&[gtk::accessible::Property::Label("Book")]);
    let book_tx = sender.clone();
    let book_sync = syncing.clone();
    let book_dd = book.clone();
    connect_list_activate(&book, move || {
        if book_sync.get() {
            return;
        }
        let pos = book_dd.selected();
        if pos != gtk::INVALID_LIST_POSITION {
            book_tx.emit(crate::app::Msg::BlankSelectBook(id, pos));
        }
    });

    let chapter = gtk::DropDown::from_strings(&[]);
    picker::prepare(&chapter, gtk::StringFilterMatchMode::Prefix);
    chapter.add_css_class("chapter-picker");
    chapter.set_tooltip_text(Some("Chapter"));
    chapter.update_property(&[gtk::accessible::Property::Label("Chapter")]);
    let chapter_tx = sender.clone();
    let chapter_sync = syncing.clone();
    let chapter_dd = chapter.clone();
    connect_list_activate(&chapter, move || {
        if chapter_sync.get() {
            return;
        }
        let pos = chapter_dd.selected();
        if pos != gtk::INVALID_LIST_POSITION {
            chapter_tx.emit(crate::app::Msg::BlankSelectChapter(id, pos));
        }
    });

    let pickers = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pickers.set_halign(gtk::Align::Start);
    pickers.append(&book);
    pickers.append(&chapter);

    let mut choices = vec![
        ("Search", None, Launch::Search),
        ("Matthew Henry", None, Launch::Mhc),
        (
            "Treasury",
            Some("Treasury of Scripture Knowledge"),
            Launch::Tsk,
        ),
    ];
    if library {
        choices.push(("Library", Some("Dictionaries and topics"), Launch::Library));
    }
    if marks {
        choices.push(("Notes", None, Launch::Notes));
        choices.push(("Bookmarks", None, Launch::Bookmarks));
    }

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for (label, tip, _) in &choices {
        let row_label = gtk::Label::new(Some(label));
        row_label.set_xalign(0.0);
        row_label.set_margin_top(10);
        row_label.set_margin_bottom(10);
        row_label.set_margin_start(12);
        row_label.set_margin_end(12);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&row_label));
        if let Some(tip) = tip {
            row.set_tooltip_text(Some(tip));
        }
        row.update_property(&[gtk::accessible::Property::Label(label)]);
        list.append(&row);
    }
    let activate = sender;
    list.connect_row_activated(move |_, row| {
        let idx = row.index();
        if idx < 0 {
            return;
        }
        let Some((_, _, choice)) = choices.get(idx as usize) else {
            return;
        };
        activate.emit(crate::app::Msg::BlankLaunch {
            id,
            choice: *choice,
        });
    });

    let open_label = gtk::Label::new(Some("Open"));
    open_label.add_css_class("dim-label");
    open_label.set_xalign(0.0);
    let open_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    open_box.append(&open_label);
    open_box.append(&list);

    let stack = gtk::Box::new(gtk::Orientation::Vertical, 24);
    stack.append(&pickers);
    stack.append(&open_box);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(420);
    clamp.set_tightening_threshold(360);
    clamp.set_child(Some(&stack));

    let root = gtk::CenterBox::new();
    root.set_orientation(gtk::Orientation::Vertical);
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.set_margin_top(32);
    root.set_margin_bottom(32);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.set_center_widget(Some(&clamp));

    BlankPage {
        root,
        book,
        chapter,
        syncing,
    }
}

/// The menu's own activate handler runs first and clears the search filter,
/// so the dropdown's selected position is the book or chapter index.
fn connect_list_activate(dropdown: &gtk::DropDown, f: impl Fn() + 'static) {
    let Some(list) = find_list(dropdown.upcast_ref()) else {
        return;
    };
    let f = Rc::new(f);
    list.connect_activate(move |_, _| {
        let f = Rc::clone(&f);
        // After this signal returns, GTK has applied the row to `selected`.
        gtk::glib::idle_add_local_once(move || f());
    });
}

fn find_list(widget: &gtk::Widget) -> Option<gtk::ListView> {
    if let Ok(list) = widget.clone().downcast::<gtk::ListView>() {
        return Some(list);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_list(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}
