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

    let choices = choices(library, marks);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for choice in &choices {
        let row = adw::ActionRow::new();
        row.set_title(choice.title);
        row.set_subtitle(choice.subtitle);
        row.set_activatable(true);
        let icon = gtk::Image::from_icon_name(choice.icon);
        icon.add_css_class("dim-label");
        row.add_prefix(&icon);
        let chevron = gtk::Image::from_icon_name("go-next-symbolic");
        chevron.add_css_class("dim-label");
        row.add_suffix(&chevron);
        list.append(&row);
    }
    let activate = sender;
    list.connect_row_activated(move |_, row| {
        let idx = row.index();
        if idx < 0 {
            return;
        }
        let Some(choice) = choices.get(idx as usize) else {
            return;
        };
        activate.emit(crate::app::Msg::BlankLaunch {
            id,
            choice: choice.launch,
        });
    });

    let read_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    read_box.append(&section_title("Read"));
    read_box.append(&pickers);

    let study_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    study_box.append(&section_title("Study"));
    study_box.append(&list);

    let stack = gtk::Box::new(gtk::Orientation::Vertical, 28);
    stack.append(&read_box);
    stack.append(&study_box);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(460);
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

/// One study view the blank tab can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: &'static str,
    pub launch: Launch,
}

/// Study views offered on a blank tab, in display order. The library and the
/// user's marks only appear when their databases are present.
pub fn choices(library: bool, marks: bool) -> Vec<Choice> {
    let mut out = vec![
        Choice {
            title: "Search",
            subtitle: "Words, phrases, and Strong’s numbers",
            icon: "system-search-symbolic",
            launch: Launch::Search,
        },
        Choice {
            title: "Matthew Henry",
            subtitle: "Commentary, verse by verse",
            icon: "x-office-document-symbolic",
            launch: Launch::Mhc,
        },
        Choice {
            title: "Treasury",
            subtitle: "Treasury of Scripture Knowledge cross-references",
            icon: "view-list-bullet-symbolic",
            launch: Launch::Tsk,
        },
    ];
    if library {
        out.push(Choice {
            title: "Library",
            subtitle: "Dictionaries, lexicons, and topics",
            icon: "accessories-dictionary-symbolic",
            launch: Launch::Library,
        });
    }
    if marks {
        out.push(Choice {
            title: "Notes",
            subtitle: "Your notes on verses",
            icon: "document-edit-symbolic",
            launch: Launch::Notes,
        });
        out.push(Choice {
            title: "Bookmarks",
            subtitle: "Verses you have saved",
            icon: "bookmark-new-symbolic",
            launch: Launch::Bookmarks,
        });
    }
    out
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    label
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_always_offer_search_and_commentaries() {
        let launches: Vec<Launch> = choices(false, false).iter().map(|c| c.launch).collect();
        assert_eq!(launches, vec![Launch::Search, Launch::Mhc, Launch::Tsk]);
    }

    #[test]
    fn choices_add_library_and_marks_when_present() {
        let launches: Vec<Launch> = choices(true, true).iter().map(|c| c.launch).collect();
        assert_eq!(
            launches,
            vec![
                Launch::Search,
                Launch::Mhc,
                Launch::Tsk,
                Launch::Library,
                Launch::Notes,
                Launch::Bookmarks,
            ]
        );
    }

    #[test]
    fn every_choice_has_a_subtitle_and_symbolic_icon() {
        for choice in choices(true, true) {
            assert!(!choice.subtitle.is_empty(), "{}", choice.title);
            assert!(choice.icon.ends_with("-symbolic"), "{}", choice.icon);
        }
    }
}
