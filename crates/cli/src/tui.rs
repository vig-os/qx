//! `pr tui` — terminal shell over the command protocol (ADR-030 §2,
//! feature `tui`).
//!
//! A thin ratatui front: a pageable entity table with status cycling +
//! free-text filter, and a detail pane. Every keystroke that needs data
//! goes through `app::dispatch` like any other shell, and **all field
//! labels come from `Describe`** — no hardcoded display strings
//! (ADR-035 §1a).
//!
//! Keys: ↑/↓ select · ⏎ detail · Esc back · s cycle status filter ·
//! / edit text filter · n/p page · r reload · q quit.

use std::io;
use std::sync::Arc;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use serde_json::Value as Json;

use part_registry_app::{dispatch, AppContext, Filter, Page, Request, Response};

const PAGE_SIZE: u32 = 25;

/// Column roster derived from the descriptor: id + status + the first
/// few declared fields (label, key). Pure — unit-tested.
fn columns_from_descriptor(descriptor: &Json) -> Vec<(String, String)> {
    let mut cols = vec![
        ("id".to_string(), "id".to_string()),
        ("status".to_string(), "status".to_string()),
    ];
    if let Some(fields) = descriptor["collections"][0]["fields"].as_array() {
        for f in fields.iter().take(4) {
            if let (Some(key), Some(label)) = (f["key"].as_str(), f["label"].as_str()) {
                cols.push((label.to_string(), key.to_string()));
            }
        }
    }
    cols
}

/// Lifecycle statuses from the descriptor (the `s` cycle order:
/// None → each status → None). Pure.
fn statuses_from_descriptor(descriptor: &Json) -> Vec<String> {
    descriptor["collections"][0]["lifecycle"]["statuses"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// One table cell value for an entity (micro-core key or declared
/// field). Pure.
fn cell_value(entity: &Json, key: &str) -> String {
    match key {
        "id" | "status" | "kind" | "created_at" => {
            entity[key].as_str().unwrap_or_default().to_string()
        }
        _ => entity["fields"][key]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }
}

struct App {
    ctx: Arc<AppContext>,
    descriptor: Json,
    columns: Vec<(String, String)>,
    statuses: Vec<String>,
    status_idx: Option<usize>,
    text: String,
    editing_text: bool,
    items: Vec<Json>,
    total: u64,
    offset: u32,
    table: TableState,
    detail: Option<Json>,
    error: Option<String>,
}

fn call(ctx: &AppContext, req: Request) -> Result<Json, String> {
    match dispatch(ctx, req) {
        Response::Ok { data, .. } => Ok(data),
        Response::Err { error, .. } => Err(format!("{:?}: {}", error.kind, error.message)),
    }
}

impl App {
    fn new(ctx: Arc<AppContext>) -> Result<Self, String> {
        let descriptor = call(&ctx, Request::Describe { collection: None })?;
        let columns = columns_from_descriptor(&descriptor);
        let statuses = statuses_from_descriptor(&descriptor);
        let mut app = Self {
            ctx,
            descriptor,
            columns,
            statuses,
            status_idx: None,
            text: String::new(),
            editing_text: false,
            items: Vec::new(),
            total: 0,
            offset: 0,
            table: TableState::default(),
            detail: None,
            error: None,
        };
        app.reload();
        Ok(app)
    }

    fn filter(&self) -> Filter {
        Filter {
            status: self.status_idx.map(|i| self.statuses[i].clone()),
            kind: None,
            text: if self.text.is_empty() {
                None
            } else {
                Some(self.text.clone())
            },
            fields: Default::default(),
        }
    }

    fn reload(&mut self) {
        let req = Request::List {
            collection: "parts".into(),
            filter: self.filter(),
            sort: Vec::new(),
            page: Page {
                offset: self.offset,
                limit: PAGE_SIZE,
            },
        };
        match call(&self.ctx, req) {
            Ok(data) => {
                self.items = data["items"].as_array().cloned().unwrap_or_default();
                self.total = data["total"].as_u64().unwrap_or(0);
                self.error = None;
                let sel = if self.items.is_empty() { None } else { Some(0) };
                self.table.select(sel);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn cycle_status(&mut self) {
        self.status_idx = match self.status_idx {
            None if !self.statuses.is_empty() => Some(0),
            Some(i) if i + 1 < self.statuses.len() => Some(i + 1),
            _ => None,
        };
        self.offset = 0;
        self.reload();
    }

    fn open_detail(&mut self) {
        if let Some(i) = self.table.selected() {
            if let Some(item) = self.items.get(i) {
                let id = item["id"].as_str().unwrap_or_default().to_string();
                match call(&self.ctx, Request::Resolve { id }) {
                    Ok(entity) => self.detail = Some(entity),
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }

    fn detail_lines(&self, entity: &Json) -> Vec<(String, String)> {
        let mut lines = vec![
            ("id".to_string(), cell_value(entity, "id")),
            ("status".to_string(), cell_value(entity, "status")),
            ("created_at".to_string(), cell_value(entity, "created_at")),
        ];
        for (status, ts) in entity["transitioned_at"]
            .as_object()
            .map(|m| m.iter().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            lines.push((
                format!("transitioned_at[{status}]"),
                ts.as_str().unwrap_or_default().to_string(),
            ));
        }
        if let Some(fields) = self.descriptor["collections"][0]["fields"].as_array() {
            for f in fields {
                if let (Some(key), Some(label)) = (f["key"].as_str(), f["label"].as_str()) {
                    let v = cell_value(entity, key);
                    if !v.is_empty() {
                        lines.push((label.to_string(), v));
                    }
                }
            }
        }
        lines
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let status_label = app
        .status_idx
        .map(|i| app.statuses[i].as_str().to_string())
        .unwrap_or_else(|| "all".into());
    let header_line = format!(
        " {} — status: {status_label} · filter: {}{} · {} of {}",
        app.descriptor["name"].as_str().unwrap_or("registry"),
        if app.text.is_empty() { "-" } else { &app.text },
        if app.editing_text { "▏" } else { "" },
        app.items.len(),
        app.total,
    );
    frame.render_widget(Paragraph::new(header_line).reversed(), layout[0]);

    if let Some(entity) = &app.detail {
        let text: Vec<Line> = app
            .detail_lines(entity)
            .into_iter()
            .map(|(label, value)| {
                Line::from(vec![
                    Span::styled(format!("{label:>22}  "), Style::new().bold()),
                    Span::raw(value),
                ])
            })
            .collect();
        frame.render_widget(
            Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
            layout[1],
        );
    } else {
        let header = Row::new(
            app.columns
                .iter()
                .map(|(label, _)| Cell::from(label.clone()))
                .collect::<Vec<_>>(),
        )
        .bold();
        let rows: Vec<Row> = app
            .items
            .iter()
            .map(|e| {
                Row::new(
                    app.columns
                        .iter()
                        .map(|(_, key)| Cell::from(cell_value(e, key)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let widths: Vec<Constraint> = app
            .columns
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i == 0 {
                    Constraint::Length(16)
                } else {
                    Constraint::Fill(1)
                }
            })
            .collect();
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().reversed());
        frame.render_stateful_widget(table, layout[1], &mut app.table);
    }

    let footer = app.error.clone().unwrap_or_else(|| {
        " ↑/↓ select · ⏎ detail · Esc back · s status · / filter · n/p page · r reload · q quit"
            .into()
    });
    frame.render_widget(Paragraph::new(footer).dim(), layout[2]);
}

/// Run the TUI until `q`. Restores the terminal on every exit path.
pub fn run(ctx: AppContext) -> Result<(), crate::CliError> {
    let mut app =
        App::new(Arc::new(ctx)).map_err(|e| crate::CliError::Other(format!("tui init: {e}")))?;

    enable_raw_mode().map_err(|e| crate::CliError::Other(format!("raw mode: {e}")))?;
    io::stdout()
        .execute(EnterAlternateScreen)
        .map_err(|e| crate::CliError::Other(format!("alt screen: {e}")))?;
    let result = (|| -> Result<(), crate::CliError> {
        let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
        let mut terminal =
            Terminal::new(backend).map_err(|e| crate::CliError::Other(format!("terminal: {e}")))?;
        loop {
            terminal
                .draw(|f| draw(f, &mut app))
                .map_err(|e| crate::CliError::Other(format!("draw: {e}")))?;
            let Event::Key(key) =
                event::read().map_err(|e| crate::CliError::Other(format!("event: {e}")))?
            else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.editing_text {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        app.editing_text = false;
                        app.offset = 0;
                        app.reload();
                    }
                    KeyCode::Backspace => {
                        app.text.pop();
                    }
                    KeyCode::Char(c) => app.text.push(c),
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Esc => app.detail = None,
                KeyCode::Enter => app.open_detail(),
                KeyCode::Char('s') => app.cycle_status(),
                KeyCode::Char('/') => app.editing_text = true,
                KeyCode::Char('r') => app.reload(),
                KeyCode::Char('n') => {
                    if u64::from(app.offset + PAGE_SIZE) < app.total {
                        app.offset += PAGE_SIZE;
                        app.reload();
                    }
                }
                KeyCode::Char('p') => {
                    app.offset = app.offset.saturating_sub(PAGE_SIZE);
                    app.reload();
                }
                KeyCode::Down => {
                    let len = app.items.len();
                    if len > 0 {
                        let next = app.table.selected().map_or(0, |i| (i + 1).min(len - 1));
                        app.table.select(Some(next));
                    }
                }
                KeyCode::Up => {
                    if !app.items.is_empty() {
                        let prev = app.table.selected().map_or(0, |i| i.saturating_sub(1));
                        app.table.select(Some(prev));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    let _ = disable_raw_mode();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn descriptor() -> Json {
        json!({
            "name": "t/r",
            "collections": [{
                "name": "parts",
                "lifecycle": { "statuses": ["unbound", "bound", "void"] },
                "fields": [
                    {"key": "type", "label": "Type"},
                    {"key": "vendor", "label": "Vendor"},
                ],
            }]
        })
    }

    #[test]
    fn columns_come_from_the_descriptor_labels() {
        let cols = columns_from_descriptor(&descriptor());
        assert_eq!(
            cols,
            vec![
                ("id".into(), "id".into()),
                ("status".into(), "status".into()),
                ("Type".into(), "type".into()),
                ("Vendor".into(), "vendor".into()),
            ]
        );
    }

    #[test]
    fn statuses_come_from_the_lifecycle() {
        assert_eq!(
            statuses_from_descriptor(&descriptor()),
            vec!["unbound", "bound", "void"]
        );
    }

    #[test]
    fn cell_values_read_micro_core_and_fields() {
        let e = json!({
            "id": "X", "status": "bound", "created_at": "2026-01-01T00:00:00Z",
            "fields": {"type": "valve"}
        });
        assert_eq!(cell_value(&e, "id"), "X");
        assert_eq!(cell_value(&e, "type"), "valve");
        assert_eq!(cell_value(&e, "vendor"), "");
    }
}
