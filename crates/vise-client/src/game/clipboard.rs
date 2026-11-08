use super::*;
use vise_client::clipboard::{self, EditShortcut};
use winit::event::KeyEvent;

impl Game<'_> {
    pub(super) fn edit_shortcut(&self, event: &KeyEvent) -> Option<EditShortcut> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let m = self.modifiers;
        let held = |keys: [KeyCode; 2]| keys.iter().any(|k| self.keys.contains(k));
        let edit = if cfg!(target_os = "macos") {
            m.super_key() || held([KeyCode::SuperLeft, KeyCode::SuperRight])
        } else {
            m.control_key() || held([KeyCode::ControlLeft, KeyCode::ControlRight])
        };
        clipboard::edit_shortcut(
            clipboard::shortcut_letter(&event.logical_key, event.physical_key),
            edit,
            m.shift_key() || held([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            m.alt_key() || held([KeyCode::AltLeft, KeyCode::AltRight]),
        )
    }
    pub(super) fn types_text(&self) -> bool {
        let held = |keys: [KeyCode; 2]| keys.iter().any(|k| self.physical_keys.contains(k));
        let edit = if cfg!(target_os = "macos") {
            held([KeyCode::SuperLeft, KeyCode::SuperRight])
        } else {
            held([KeyCode::ControlLeft, KeyCode::ControlRight])
        };
        clipboard::types_text(
            edit,
            held([KeyCode::AltLeft, KeyCode::AltRight]),
            clipboard::ALTGR_IS_CTRL_ALT,
        )
    }
    pub(super) fn append_shortcut(
        selection: &mut clipboard::AppendSelection,
        board: &mut clipboard::SystemClipboard,
        id: clipboard::FieldId,
        value: &mut String,
        shortcut: EditShortcut,
        max_chars: usize,
    ) -> bool {
        selection.apply(id, value, shortcut, board, |v, t| {
            clipboard::append_limited(v, t, max_chars)
        })
    }
    pub(super) fn title_shortcut(&mut self, shortcut: EditShortcut) {
        let Some(title) = self.title.as_mut() else {
            return;
        };
        if title.has_caret_field() {
            return title.field_shortcut(shortcut, &mut self.clipboard);
        }
        if let Some((id, value, max)) = title.focused_field() {
            let changed = Self::append_shortcut(
                &mut self.text_selection,
                &mut self.clipboard,
                id,
                value,
                shortcut,
                max,
            );
            if changed {
                title.search_changed();
            }
        }
    }
    pub(super) fn title_edit(&mut self, typed: Option<&str>, backspace: bool) {
        let Some(title) = self.title.as_mut() else {
            return;
        };
        let cleared = !title.has_caret_field()
            && title
                .focused_field()
                .is_some_and(|(id, value, _)| self.text_selection.take(id, value));
        if cleared {
            title.search_changed();
        }
        if backspace {
            if !cleared {
                title.backspace();
            }
        } else if let Some(text) = typed {
            title.type_text(text);
        }
    }
    pub(super) fn creative_search_shortcut(&mut self, shortcut: EditShortcut) {
        if self
            .creative_screen
            .search
            .shortcut(shortcut, &mut self.clipboard)
        {
            self.creative_screen.scroll = 0.0;
        }
    }
    pub(super) fn creative_search_backspace(&mut self, ctrl: bool) {
        self.creative_screen
            .search
            .key(KeyCode::Backspace, ctrl, false);
        self.creative_screen.scroll = 0.0;
    }
    pub(super) fn creative_search_edit(&mut self, typed: Option<&str>) {
        if let Some(text) = typed {
            self.creative_screen.search.insert_text(text);
            self.creative_screen.scroll = 0.0;
        }
    }
}

pub(super) const CREATIVE_SEARCH_CHARS: usize = 50;
