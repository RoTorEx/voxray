use std::io::{self, stdout};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};

use crate::workflow::Stage;

const ACCENT: Color = Color::Rgb(94, 234, 212);
const SELECTED_BG: Color = Color::Rgb(45, 212, 191);
const SELECTED_FG: Color = Color::Rgb(8, 47, 73);
const TEXT: Color = Color::Rgb(248, 250, 252);
const MUTED: Color = Color::Rgb(203, 213, 225);

pub struct LaunchSelection {
    pub profile: String,
    pub through: Option<Stage>,
    pub edit_profile: bool,
}

struct LaunchMenu {
    command: Stage,
    profiles: Vec<String>,
    profile_index: usize,
    through: Vec<Option<Stage>>,
    through_index: usize,
    show_through: bool,
    edit_profile: bool,
    show_settings: bool,
    field: usize,
}

impl LaunchMenu {
    fn new(
        command: Stage,
        profiles: Vec<String>,
        preferred_profile: Option<&str>,
        preferred_through: Option<Stage>,
        preferred_edit_profile: bool,
    ) -> Result<Self> {
        let profile_index = match preferred_profile {
            Some(name) => profiles
                .iter()
                .position(|profile| profile == name)
                .with_context(|| format!("Profile '{name}' not found"))?,
            None => 0,
        };
        let through = through_choices(command);
        let through_index = through
            .iter()
            .position(|stage| *stage == preferred_through)
            .context("Selected --through stage is unavailable for this command")?;
        let show_through = through.len() > 1 && preferred_through.is_none();
        Ok(Self {
            command,
            profiles,
            profile_index,
            through,
            through_index,
            show_through,
            edit_profile: preferred_edit_profile,
            show_settings: !preferred_edit_profile,
            field: 0,
        })
    }

    fn field_count(&self) -> usize {
        1 + usize::from(self.show_through) + usize::from(self.show_settings)
    }

    fn has_through_field(&self) -> bool {
        self.show_through
    }

    fn move_field(&mut self, delta: isize) {
        self.field = cycle_index(self.field, self.field_count(), delta);
    }

    fn change_value(&mut self, delta: isize) {
        if self.field == 0 {
            self.profile_index = cycle_index(self.profile_index, self.profiles.len(), delta);
        } else if self.has_through_field() && self.field == 1 {
            self.through_index = cycle_index(self.through_index, self.through.len(), delta);
        } else if self.show_settings {
            self.edit_profile = !self.edit_profile;
        }
    }

    fn selection(&self) -> LaunchSelection {
        LaunchSelection {
            profile: self.profiles[self.profile_index].clone(),
            through: self.through[self.through_index],
            edit_profile: self.edit_profile,
        }
    }
}

pub fn run(
    command: Stage,
    profiles: Vec<String>,
    preferred_profile: Option<&str>,
    preferred_through: Option<Stage>,
    preferred_edit_profile: bool,
) -> Result<LaunchSelection> {
    let mut menu = LaunchMenu::new(
        command,
        profiles,
        preferred_profile,
        preferred_through,
        preferred_edit_profile,
    )?;
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    loop {
        terminal.draw(|frame| draw(frame, &menu))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc => bail!("Interactive setup cancelled"),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    bail!("Interactive setup cancelled")
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => menu.move_field(-1),
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => menu.move_field(1),
                KeyCode::Left | KeyCode::Char('h') => menu.change_value(-1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => menu.change_value(1),
                KeyCode::Enter => return Ok(menu.selection()),
                _ => {}
            }
        }
    }
}

fn through_choices(command: Stage) -> Vec<Option<Stage>> {
    match command {
        Stage::Inbox => vec![None, Some(Stage::Transcribe), Some(Stage::Feedback)],
        Stage::Transcribe => vec![None, Some(Stage::Feedback)],
        Stage::Feedback => vec![None],
    }
}

fn cycle_index(current: usize, length: usize, delta: isize) -> usize {
    (current as isize + delta).rem_euclid(length as isize) as usize
}

fn draw(frame: &mut ratatui::Frame, menu: &LaunchMenu) {
    let area = centered_rect(60, 10, frame.area());
    frame.render_widget(Clear, area);
    let title = format!(" ◆ Voxray · {} ", title_case(menu.command.as_str()));
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new("Choose settings, then run")
            .style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        rows[0],
    );
    draw_field(
        frame,
        rows[1],
        "Profile",
        &menu.profiles[menu.profile_index],
        menu.field == 0,
    );
    let mut next_row = 2;
    let mut next_field = 1;
    if menu.has_through_field() {
        let through = menu.through[menu.through_index]
            .map(Stage::as_str)
            .unwrap_or("none");
        draw_field(
            frame,
            rows[next_row],
            "Through",
            through,
            menu.field == next_field,
        );
        next_row += 1;
        next_field += 1;
    }
    if menu.show_settings {
        draw_field(
            frame,
            rows[next_row],
            "Settings",
            if menu.edit_profile {
                "review & edit"
            } else {
                "use profile"
            },
            menu.field == next_field,
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Enter",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" run", Style::default().fg(TEXT)),
            Span::styled("  ·  ", Style::default().fg(MUTED)),
            Span::styled(
                "Esc",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(TEXT)),
        ])),
        rows[6],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(ACCENT)),
            Span::styled(" field", Style::default().fg(MUTED)),
            Span::styled("  ·  ", Style::default().fg(MUTED)),
            Span::styled("←→", Style::default().fg(ACCENT)),
            Span::styled(" value", Style::default().fg(MUTED)),
        ])),
        rows[7],
    );
}

fn draw_field(frame: &mut ratatui::Frame, area: Rect, label: &str, value: &str, selected: bool) {
    let (row_style, label_style, value_style, arrow_style) = if selected {
        let selected = Style::default()
            .fg(SELECTED_FG)
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD);
        (selected, selected, selected, selected)
    } else {
        (
            Style::default(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            Style::default().fg(TEXT),
            Style::default().fg(MUTED),
        )
    };
    let marker = if selected { "›" } else { " " };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{marker} {label:<9}"), label_style),
            Span::styled("‹ ", arrow_style),
            Span::styled(value, value_style),
            Span::styled(" ›", arrow_style),
        ]))
        .style(row_style),
        area,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height.saturating_sub(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("Failed to enable terminal raw mode")?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, cursor::Hide) {
            let _ = disable_raw_mode();
            return Err(error).context("Failed to open interactive screen");
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), cursor::Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(command: Stage) -> LaunchMenu {
        LaunchMenu::new(
            command,
            vec!["default".to_string(), "sales".to_string()],
            None,
            None,
            false,
        )
        .unwrap()
    }

    #[test]
    fn defaults_to_default_profile_and_no_pipeline() {
        let menu = menu(Stage::Inbox);
        let selection = menu.selection();

        assert_eq!(selection.profile, "default");
        assert_eq!(selection.through, None);
        assert!(!selection.edit_profile);
    }

    #[test]
    fn navigation_changes_profile_and_through_without_extra_dialogs() {
        let mut menu = menu(Stage::Inbox);
        menu.change_value(1);
        menu.move_field(1);
        menu.change_value(1);
        let selection = menu.selection();

        assert_eq!(selection.profile, "sales");
        assert_eq!(selection.through, Some(Stage::Transcribe));
    }

    #[test]
    fn cli_flags_preselect_values_in_the_menu() {
        let menu = LaunchMenu::new(
            Stage::Inbox,
            vec!["default".to_string(), "sales".to_string()],
            Some("sales"),
            Some(Stage::Feedback),
            true,
        )
        .unwrap();
        let selection = menu.selection();

        assert_eq!(selection.profile, "sales");
        assert_eq!(selection.through, Some(Stage::Feedback));
        assert!(selection.edit_profile);
        assert_eq!(menu.field_count(), 1);
    }

    #[test]
    fn feedback_has_no_irrelevant_through_field() {
        assert_eq!(menu(Stage::Feedback).field_count(), 2);
    }

    #[test]
    fn explicit_through_is_not_shown_as_an_interactive_field() {
        let menu = LaunchMenu::new(
            Stage::Inbox,
            vec!["default".to_string(), "sales".to_string()],
            None,
            Some(Stage::Transcribe),
            false,
        )
        .unwrap();

        assert!(!menu.has_through_field());
        assert_eq!(menu.field_count(), 2);
        assert_eq!(menu.selection().through, Some(Stage::Transcribe));
    }

    #[test]
    fn explicit_edit_profile_is_not_shown_as_an_interactive_field() {
        let menu = LaunchMenu::new(
            Stage::Inbox,
            vec!["default".to_string(), "sales".to_string()],
            None,
            None,
            true,
        )
        .unwrap();

        assert!(!menu.show_settings);
        assert_eq!(menu.field_count(), 2);
        assert!(menu.selection().edit_profile);
    }

    #[test]
    fn settings_row_enables_profile_review() {
        let mut menu = menu(Stage::Inbox);
        menu.move_field(1);
        menu.move_field(1);
        menu.change_value(1);

        assert!(menu.selection().edit_profile);
    }

    #[test]
    fn selected_row_uses_explicit_high_contrast_colors() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw(frame, &menu(Stage::Inbox)))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let selected = buffer
            .content
            .iter()
            .find(|cell| cell.symbol() == "›")
            .expect("selected row marker");

        assert_eq!(selected.fg, SELECTED_FG);
        assert_eq!(selected.bg, SELECTED_BG);
    }
}
