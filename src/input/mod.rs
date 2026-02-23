use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::core::Command;

pub fn map_key_event(key: KeyEvent, search_mode: bool) -> Option<Command> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    let mods = key.modifiers;
    match (key.code, mods) {
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('Q'), KeyModifiers::CONTROL) => Some(Command::Quit),
        (KeyCode::Char('s'), KeyModifiers::CONTROL)
        | (KeyCode::Char('S'), KeyModifiers::CONTROL) => Some(Command::Save),
        (KeyCode::Char('s'), m) | (KeyCode::Char('S'), m)
            if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
        {
            Some(Command::Save)
        }
        (KeyCode::F(1), KeyModifiers::NONE) => Some(Command::OpenHelp),
        (KeyCode::Esc, _) => Some(Command::CloseOverlay),
        (KeyCode::Char('f'), KeyModifiers::CONTROL)
        | (KeyCode::Char('F'), KeyModifiers::CONTROL) => Some(Command::OpenSearch),
        (KeyCode::Char('g'), KeyModifiers::CONTROL)
        | (KeyCode::Char('G'), KeyModifiers::CONTROL) => Some(Command::OpenGoto),
        (KeyCode::Char('o'), KeyModifiers::CONTROL)
        | (KeyCode::Char('O'), KeyModifiers::CONTROL) => Some(Command::OpenFileTree),
        (KeyCode::Char('n'), KeyModifiers::CONTROL)
        | (KeyCode::Char('N'), KeyModifiers::CONTROL) => Some(Command::NewFile),
        (KeyCode::Enter, m) if search_mode && m.contains(KeyModifiers::SHIFT) => {
            Some(Command::SearchPrev)
        }
        (KeyCode::Enter, _) if search_mode => Some(Command::SearchNext),
        (KeyCode::Left, _) => Some(Command::MoveLeft),
        (KeyCode::Right, _) => Some(Command::MoveRight),
        (KeyCode::Up, _) => Some(Command::MoveUp),
        (KeyCode::Down, _) => Some(Command::MoveDown),
        (KeyCode::Home, _) => Some(Command::MoveHome),
        (KeyCode::End, _) => Some(Command::MoveEnd),
        (KeyCode::PageUp, _) => Some(Command::PageUp),
        (KeyCode::PageDown, _) => Some(Command::PageDown),
        (KeyCode::Backspace, _) => Some(Command::Backspace),
        (KeyCode::Delete, _) => Some(Command::Delete),
        (KeyCode::Enter, _) => Some(Command::NewLine),
        (KeyCode::Tab, _) => Some(Command::Insert('\t')),
        (KeyCode::Char(c), KeyModifiers::NONE) => Some(Command::Insert(c)),
        (KeyCode::Char(c), KeyModifiers::SHIFT) => Some(Command::Insert(c)),
        (KeyCode::F(10), KeyModifiers::NONE) => Some(Command::ResetLineColor),
        (KeyCode::F(2), KeyModifiers::NONE) => Some(Command::SetLineColor(1)),
        (KeyCode::F(3), KeyModifiers::NONE) => Some(Command::SetLineColor(2)),
        (KeyCode::F(4), KeyModifiers::NONE) => Some(Command::SetLineColor(3)),
        (KeyCode::F(5), KeyModifiers::NONE) => Some(Command::SetLineColor(4)),
        (KeyCode::F(6), KeyModifiers::NONE) => Some(Command::SetLineColor(5)),
        (KeyCode::F(7), KeyModifiers::NONE) => Some(Command::SetLineColor(6)),
        (KeyCode::F(8), KeyModifiers::NONE) => Some(Command::SetLineColor(7)),
        (KeyCode::F(9), KeyModifiers::NONE) => Some(Command::SetLineColor(8)),
        _ => None,
    }
}
