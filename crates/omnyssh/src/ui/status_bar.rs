use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{AppState, FileManagerPopup, Screen, SnippetPopup, ViewState};

/// Renders the bottom status bar with context-sensitive key hints.
pub fn render(frame: &mut Frame, area: Rect, state: &AppState, view: &ViewState) {
    // Apply active theme colours.
    let key_style = Style::default()
        .fg(view.theme.key_badge_fg)
        .bg(view.theme.key_badge_bg)
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(view.theme.separator_fg);
    let hint_style = Style::default().fg(view.theme.hint_fg);

    macro_rules! key {
        ($k:expr) => {
            Span::styled(format!(" {} ", $k), key_style)
        };
    }
    macro_rules! hint {
        ($h:expr) => {
            Span::styled(format!(" {} ", $h), hint_style)
        };
    }
    macro_rules! sep {
        () => {
            Span::styled(" │ ", sep_style)
        };
    }

    // If a status message is set, show it instead of key hints.
    if let Some(msg) = &view.status_message {
        let line = Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::Yellow),
        ));
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    let hlv = &view.host_list;

    // Search mode: show search-specific hints.
    if hlv.search_mode {
        let line = Line::from(vec![
            Span::styled(
                format!(" [{}] ", crate::i18n::tr("status-search")),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}  ", crate::i18n::tr("status-type-filter")),
                Style::default().fg(Color::Gray),
            ),
            key!("Enter"),
            hint!(crate::i18n::tr("common-confirm")),
            sep!(),
            key!("Esc"),
            hint!(crate::i18n::tr("status-clear")),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    // Popup mode: show popup-specific hints.
    if let Some(popup) = &hlv.popup {
        use crate::app::HostPopup;
        let line = match popup {
            HostPopup::Add(_) | HostPopup::Edit { .. } => Line::from(vec![
                key!("Tab"),
                hint!(crate::i18n::tr("status-next-field")),
                sep!(),
                key!("Shift+Tab"),
                hint!(crate::i18n::tr("status-prev-field")),
                sep!(),
                key!("Enter"),
                hint!(crate::i18n::tr("common-save")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            HostPopup::DeleteConfirm(_) => Line::from(vec![
                key!("y"),
                hint!(crate::i18n::tr("status-confirm-delete")),
                sep!(),
                key!("n / Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            HostPopup::KeySetupConfirm(_) => Line::from(vec![
                key!("y / Enter"),
                hint!(crate::i18n::tr("status-confirm-setup")),
                sep!(),
                key!("n / Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            HostPopup::KeySetupProgress { .. } => Line::from(vec![
                Span::styled(
                    format!(" {} ", crate::i18n::tr("status-key-setup")),
                    hint_style,
                ),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-close")),
            ]),
        };
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    // Snippets search mode.
    if view.snippets_view.search_mode {
        let line = Line::from(vec![
            Span::styled(
                format!(" [{}] ", crate::i18n::tr("status-search")),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}  ", crate::i18n::tr("status-type-filter")),
                Style::default().fg(Color::Gray),
            ),
            key!("Enter"),
            hint!(crate::i18n::tr("common-confirm")),
            sep!(),
            key!("Esc"),
            hint!(crate::i18n::tr("status-clear")),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    // Snippets popup hints.
    if let Some(popup) = &view.snippets_view.popup {
        let line = match popup {
            SnippetPopup::Add(_) | SnippetPopup::Edit { .. } => Line::from(vec![
                key!("Tab"),
                hint!(crate::i18n::tr("status-next-field")),
                sep!(),
                key!("Shift+Tab"),
                hint!(crate::i18n::tr("status-prev-field")),
                sep!(),
                key!("Enter"),
                hint!(crate::i18n::tr("common-save")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            SnippetPopup::DeleteConfirm(_) => Line::from(vec![
                key!("y"),
                hint!(crate::i18n::tr("status-confirm-delete")),
                sep!(),
                key!("n / Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            SnippetPopup::ParamInput { .. } => Line::from(vec![
                key!("Tab"),
                hint!(crate::i18n::tr("status-next-param")),
                sep!(),
                key!("Enter"),
                hint!(crate::i18n::tr("snippets-run")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            SnippetPopup::BroadcastPicker { .. } => Line::from(vec![
                key!("j/k"),
                hint!(crate::i18n::tr("status-navigate")),
                sep!(),
                key!("Space"),
                hint!(crate::i18n::tr("status-toggle")),
                sep!(),
                key!("Enter"),
                hint!(crate::i18n::tr("snippets-run")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            SnippetPopup::QuickExecuteInput { .. } => Line::from(vec![
                key!("Enter"),
                hint!(crate::i18n::tr("snippets-run")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-cancel")),
            ]),
            SnippetPopup::Results { .. } => Line::from(vec![
                key!("j/k"),
                hint!(crate::i18n::tr("status-scroll")),
                sep!(),
                key!("Esc"),
                hint!(crate::i18n::tr("common-close")),
            ]),
        };
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    // File manager popup hints.
    if matches!(state.screen, Screen::FileManager) {
        if let Some(fm_popup) = &view.file_manager.popup {
            let line = match fm_popup {
                FileManagerPopup::HostPicker { .. } => Line::from(vec![
                    key!("j/k"),
                    hint!(crate::i18n::tr("status-navigate")),
                    sep!(),
                    key!("Enter"),
                    hint!(crate::i18n::tr("common-connect")),
                    sep!(),
                    key!("Esc"),
                    hint!(crate::i18n::tr("common-cancel")),
                ]),
                FileManagerPopup::DeleteConfirm { .. } => Line::from(vec![
                    key!("y"),
                    hint!(crate::i18n::tr("status-confirm-delete")),
                    sep!(),
                    key!("n / Esc"),
                    hint!(crate::i18n::tr("common-cancel")),
                ]),
                FileManagerPopup::MkDir(_) | FileManagerPopup::Rename { .. } => Line::from(vec![
                    key!("Enter"),
                    hint!(crate::i18n::tr("common-confirm")),
                    sep!(),
                    key!("Esc"),
                    hint!(crate::i18n::tr("common-cancel")),
                ]),
                FileManagerPopup::TransferProgress {
                    filename,
                    done,
                    total,
                    ..
                } => {
                    let pct = if *total > 0 {
                        ((*done as f64 / *total as f64) * 100.0) as u64
                    } else {
                        0
                    };
                    Line::from(vec![Span::styled(
                        crate::i18n::tr_args(
                            "status-transferring",
                            &[("filename", filename.clone()), ("percent", pct.to_string())],
                        ),
                        hint_style,
                    )])
                }
            };
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(Color::Reset)),
                area,
            );
            return;
        }
    }

    // Tag popup: show its own hints.
    if hlv.tag_popup_open {
        let line = Line::from(vec![
            key!("j/k"),
            hint!(crate::i18n::tr("status-navigate")),
            sep!(),
            key!("Enter"),
            hint!(crate::i18n::tr("common-select")),
            sep!(),
            key!("Esc"),
            hint!(crate::i18n::tr("common-close")),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Reset)),
            area,
        );
        return;
    }

    // Global hints only - screen-specific hints are now in page headers.
    let spans = vec![
        key!("1"),
        hint!(crate::i18n::tr("screen-dashboard")),
        sep!(),
        key!("2"),
        hint!(crate::i18n::tr("screen-files")),
        sep!(),
        key!("3"),
        hint!(crate::i18n::tr("screen-snippets")),
        sep!(),
        key!("4"),
        hint!(crate::i18n::tr("screen-terminal")),
        sep!(),
        key!("?"),
        hint!(crate::i18n::tr("help-title")),
        sep!(),
        key!("q"),
        hint!(crate::i18n::tr("status-quit")),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Reset)),
        area,
    );
}
