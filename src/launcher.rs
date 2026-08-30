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

pub struct LaunchSelection {
    pub profile: String,
    pub through: Option<Stage>,
}

struct LaunchMenu {
    command: Stage,
    profiles: Vec<String>,
    profile_index: usize,
    through: Vec<Option<Stage>>,
    through_index: usize,
    field: usize,
}

impl LaunchMenu {
    fn new(
        command: Stage,
        profiles: Vec<String>,
        preferred_profile: Option<&str>,
        preferred_through: Option<Stage>,
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
        Ok(Self {
            command,
            profiles,
            profile_index,
            through,
            through_index,
            field: 0,
        })
    }

    fn field_count(&self) -> usize {
        if self.through.len() > 1 { 2 } else { 1 }
    }

    fn move_field(&mut self, delta: isize) {
        self.field = cycle_index(self.field, self.field_count(), delta);
    }

    fn change_value(&mut self, delta: isize) {
        if self.field == 0 {
            self.profile_index = cycle_index(self.profile_index, self.profiles.len(), delta);
        } else {
            self.through_index = cycle_index(self.through_index, self.through.len(), delta);
        }
    }

    fn selection(&self) -> LaunchSelection {
        LaunchSelection {
            profile: self.profiles[self.profile_index].clone(),
            through: self.through[self.through_index],
        }
    }
}

pub fn run(
    command: Stage,
    profiles: Vec<String>,
    preferred_profile: Option<&str>,
    preferred_through: Option<Stage>,
) -> Result<LaunchSelection> {
    let mut menu = LaunchMenu::new(command, profiles, preferred_profile, preferred_through)?;
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
    let area = centered_rect(
        64,
        if menu.field_count() == 2 { 12 } else { 10 },
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let title = format!(" ◆ Voxray · {} ", title_case(menu.command.as_str()));
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .padding(Padding::horizontal(2));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new("Configure this run").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        rows[0],
    );
    draw_field(
        frame,
        rows[1],
        "Profile",
        &menu.profiles[menu.profile_index],
        menu.field == 0,
    );
    if menu.field_count() == 2 {
        let through = menu.through[menu.through_index]
            .map(Stage::as_str)
            .unwrap_or("none");
        draw_field(frame, rows[2], "Through", through, menu.field == 1);
    }
    frame.render_widget(
        Paragraph::new("Defaults are ready — Enter starts the command")
            .style(Style::default().fg(Color::DarkGray)),
        rows[4],
    );

    let help = "↑↓ field  •  ←→ value  •  Enter continue  •  Esc cancel";
    let help_area = Rect::new(
        frame.area().x,
        area.y.saturating_add(area.height),
        frame.area().width,
        1,
    );
    frame.render_widget(
        Paragraph::new(help)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        help_area,
    );
}

fn draw_field(frame: &mut ratatui::Frame, area: Rect, label: &str, value: &str, selected: bool) {
    let style = if selected {
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let marker = if selected { "›" } else { " " };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{marker} {label:<9}"), style),
            Span::styled(format!("‹ {value} › "), style),
        ]))
        .style(style),
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
        )
        .unwrap()
    }

    #[test]
    fn defaults_to_default_profile_and_no_pipeline() {
        let menu = menu(Stage::Inbox);
        let selection = menu.selection();

        assert_eq!(selection.profile, "default");
        assert_eq!(selection.through, None);
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
        )
        .unwrap();
        let selection = menu.selection();

        assert_eq!(selection.profile, "sales");
        assert_eq!(selection.through, Some(Stage::Feedback));
    }

    #[test]
    fn feedback_has_no_irrelevant_through_field() {
        assert_eq!(menu(Stage::Feedback).field_count(), 1);
    }
}
