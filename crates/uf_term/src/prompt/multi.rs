//! Optional checkbox selection, using the same terminal and filter as `select`.

use super::key::{Key, read_key};
use super::menu::Menu;
use super::raw::RawMode;
use super::{Choice, Keys, Request, draw, erase, is_interactive, redraw_until};
use crate::capability::{Capabilities, TerminalEnv};
use crate::theme::Theme;
use std::io;

/// How an optional multiple choice prompt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManyOutcome<'a> {
    /// The checked choices, in their original order. Empty means skip.
    Chose(Vec<&'a Choice<'a>>),
    /// The reader cancelled or input closed.
    Cancelled,
    /// No interactive terminal was available.
    NotInteractive,
}

struct Checks<'a> {
    menu: Menu<'a>,
    choices: &'a [Choice<'a>],
    checked: Vec<bool>,
}
impl<'a> Checks<'a> {
    fn new(choices: &'a [Choice<'a>]) -> Self {
        Self {
            menu: Menu::new(choices),
            choices,
            checked: vec![false; choices.len()],
        }
    }
    fn press(&mut self, key: Key) -> Option<ManyOutcome<'a>> {
        match key {
            Key::Escape => return Some(ManyOutcome::Cancelled),
            Key::Enter => {
                return Some(ManyOutcome::Chose(
                    self.choices
                        .iter()
                        .zip(&self.checked)
                        .filter_map(|(choice, checked)| checked.then_some(choice))
                        .collect(),
                ));
            }
            Key::Char(' ') => {
                if let Some(choice) = self.menu.selected() {
                    let index = self
                        .choices
                        .iter()
                        .position(|item| std::ptr::eq(item, choice))
                        .unwrap();
                    self.checked[index] = !self.checked[index];
                }
            }
            key => {
                super::press(&mut self.menu, key);
            }
        }
        None
    }
    fn draw(&self, frame: &draw::Frame<'_>, out: &mut String) {
        draw::multi_frame(&self.menu, frame, out, &|choice| {
            self.choices
                .iter()
                .position(|item| std::ptr::eq(item, choice))
                .is_some_and(|index| self.checked[index])
        });
    }
}

/// Choose zero or more items: Space toggles, Enter confirms, Escape cancels.
/// Selection survives filtering and does not read from redirected input.
pub fn select_many<'a>(request: &Request<'a>) -> ManyOutcome<'a> {
    if !is_interactive() {
        return ManyOutcome::NotInteractive;
    }
    let Ok(raw) = RawMode::enter() else {
        return ManyOutcome::NotInteractive;
    };
    let theme = Theme::default();
    let frame = draw::Frame {
        title: request.title,
        placeholder: request.placeholder,
        capabilities: Capabilities::for_stderr(request.color, &TerminalEnv::from_process()),
        theme: &theme,
    };
    let mut checks = Checks::new(request.choices);
    let mut keys = || read_key(&raw);
    let mut out = io::stderr();
    let mut paint = |state: &Checks<'a>, buffer: &mut String| state.draw(&frame, buffer);
    let keys: &mut Keys<'_> = &mut keys;
    let answer = redraw_until(&mut checks, keys, &mut paint, &mut out, Checks::press)
        .unwrap_or(ManyOutcome::Cancelled);
    erase(&checks, &mut paint, &mut out);
    drop(raw);
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_items_survive_filtering_and_are_returned_in_original_order() {
        let choices = [
            Choice::new("vscode", "Code"),
            Choice::new("zed", "Zed"),
            Choice::new("helix", "Helix"),
        ];
        let mut state = Checks::new(&choices);
        for key in [
            Key::Down,
            Key::Char(' '),
            Key::Char('h'),
            Key::Char(' '),
            Key::ClearLine,
            Key::Char(' '),
        ] {
            state.press(key);
        }
        assert_eq!(
            state.press(Key::Enter),
            Some(ManyOutcome::Chose(choices.iter().collect()))
        );
    }
    #[test]
    fn empty_selection_skips_and_toggling_twice_removes_selection() {
        let choices = [Choice::new("vscode", "Code")];
        let mut state = Checks::new(&choices);
        assert_eq!(state.press(Key::Enter), Some(ManyOutcome::Chose(vec![])));
        state.press(Key::Char(' '));
        state.press(Key::Char(' '));
        assert_eq!(state.press(Key::Enter), Some(ManyOutcome::Chose(vec![])));
        assert_eq!(state.press(Key::Escape), Some(ManyOutcome::Cancelled));
    }
}
