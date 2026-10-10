//! The GUI popup: one process per tool call, one question on screen at a time,
//! borderless always-on-top window, lives exactly as long as the questions do.
//!
//! Keyboard: ↑/↓ move the option cursor, Enter commits the current question
//! (custom text > highlighted option > ticked options) and advances, Esc cancels.
//!
//! Visual concept: a quiet full-bleed menu list, not a web form — a thin
//! progress bar for multi-question runs, an uppercase topic label over the
//! question, plain rows whose selection is shown by an accent radio/checkbox
//! and label color, and one nav row at the bottom. Type scale: 10/12/15/18.

use crate::spec::{Answer, AnswerStatus, PopupSpec, Question, QuestionAnswer};
use crate::theme;
use iced::alignment::Vertical;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_editor};
use iced::{keyboard, time, window, Border, Element, Length, Size, Subscription, Task, Theme};
use std::io::Write;
use std::time::Duration;

/// Window width; rows and the progress bar derive from it.
const WIN_W: f32 = 400.0;
const PAD_X: f32 = 20.0;
/// Usable content width (window minus horizontal padding).
const CONTENT_W: f32 = WIN_W - 2.0 * PAD_X;

pub fn run_popup() {
    match try_run_popup() {
        Ok(()) => {}
        Err(e) => {
            write_answer(Answer {
                status: AnswerStatus::Error,
                answers: vec![],
                error: Some(e),
            });
            std::process::exit(1);
        }
    }
}

fn try_run_popup() -> Result<(), String> {
    // Rendering is pure software (tiny_skia — iced built without wgpu): the
    // NVIDIA proprietary driver dropped text layers on GL and froze frame
    // presentation on Vulkan, so the GPU is deliberately out of the picture.

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("failed to read question from stdin: {e}"))?;
    let spec: PopupSpec = serde_json::from_str(line.trim())
        .map_err(|e| format!("invalid question JSON on stdin: {e}"))?;
    if spec.questions.is_empty() {
        return Err("popup received no questions".into());
    }

    let settings = window_settings(&spec);
    // BootFn needs `Fn`, so the closure clones its captures per call.
    // Deliberately NO autofocus: the note editor is multiline, so when focused
    // it would eat ArrowUp/Down and break keyboard option navigation.
    let boot_spec = spec.clone();
    iced::application(
        move || (Popup::new(boot_spec.clone()), Task::none()),
        update,
        view,
    )
    .subscription(subscription)
    .theme(|_popup: &Popup| Theme::Dark)
    .window(settings)
    .title("aski")
    .run()
    .map_err(|e| format!("failed to run GUI: {e}"))
}

/// Per-question answer state.
struct QuestionState {
    selected: Vec<bool>,
    /// Free-form note for THIS question; Enter = newline, Shift+Enter sends.
    note: text_editor::Content,
}

struct Popup {
    spec: PopupSpec,
    palette: theme::Palette,
    states: Vec<QuestionState>,
    /// Index of the question currently on screen.
    current: usize,
    /// Keyboard-focused option within the current question. Cleared on mouse
    /// clicks — the plate highlight is a keyboard aid, not a selection state.
    cursor: Option<usize>,
    remaining: Option<u64>,
}

impl Popup {
    fn new(spec: PopupSpec) -> Self {
        let flavor = spec.flavor.unwrap_or_default();
        let palette = theme::resolve(flavor, spec.accent.as_deref());
        let remaining = match spec.timeout_secs {
            Some(0) | None => None,
            Some(secs) => Some(secs),
        };
        let states = spec
            .questions
            .iter()
            .map(|q| QuestionState {
                selected: vec![false; q.options.len()],
                note: text_editor::Content::new(),
            })
            .collect();
        Self {
            spec,
            palette,
            states,
            current: 0,
            cursor: None,
            remaining,
        }
    }

    fn question(&self) -> &Question {
        &self.spec.questions[self.current]
    }

    fn state(&self) -> &QuestionState {
        &self.states[self.current]
    }

    fn state_mut(&mut self) -> &mut QuestionState {
        &mut self.states[self.current]
    }

    fn topic_label(&self) -> String {
        let n = self.spec.questions.len();
        let header = self.spec.questions[self.current]
            .header
            .clone()
            .unwrap_or_else(|| "Decision needed".into())
            .to_uppercase();
        if n > 1 {
            format!("{header}  ·  {} / {}", self.current + 1, n)
        } else {
            header
        }
    }

    /// Resolve the final selections for question `idx`: ticked options plus
    /// this question's free-form note appended at the end (the note never
    /// replaces a selection); with nothing ticked, the note stands alone;
    /// single-select only: otherwise the highlighted option.
    fn collect(&self, idx: usize) -> Vec<String> {
        let st = &self.states[idx];
        let q = &self.spec.questions[idx];
        let text = st.note.text();
        let custom = text.trim();
        let mut out: Vec<String> = st
            .selected
            .iter()
            .enumerate()
            .filter(|(_, checked)| **checked)
            .map(|(i, _)| q.options[i].label.clone())
            .collect();
        if !out.is_empty() {
            if !custom.is_empty() {
                out.push(custom.to_string());
            }
            return out;
        }
        if !custom.is_empty() {
            return vec![custom.to_string()];
        }
        if !q.multi_select {
            if let Some(cursor) = self.cursor {
                if let Some(opt) = q.options.get(cursor) {
                    return vec![opt.label.clone()];
                }
            }
        }
        vec![]
    }
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Toggle(usize, bool),
    MoveCursor(i32),
    Next,
    Back,
    Edit(text_editor::Action),
    Tick,
    Close,
}

fn finish(answer: Answer) -> ! {
    write_answer(answer);
    std::process::exit(0);
}

fn write_answer(answer: Answer) {
    if let Ok(json) = serde_json::to_string(&answer) {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    }
}

fn update(popup: &mut Popup, message: Message) -> Task<Message> {
    match message {
        Message::Select(i) => {
            // Single-select: clicking marks the option; Next/Enter commits.
            // The cursor follows the click so the preview panel tracks what
            // the user is looking at — same for the keyboard.
            if popup.question().options.get(i).is_some() {
                for (j, slot) in popup.state_mut().selected.iter_mut().enumerate() {
                    *slot = j == i;
                }
                popup.cursor = Some(i);
            }
            Task::none()
        }
        Message::Toggle(i, value) => {
            if let Some(slot) = popup.state_mut().selected.get_mut(i) {
                *slot = value;
            }
            popup.cursor = Some(i);
            Task::none()
        }
        Message::MoveCursor(delta) => {
            let max = popup.question().options.len().saturating_sub(1);
            let base = popup.cursor.map_or(0, |c| c as i32);
            popup.cursor = Some((base + delta).clamp(0, max as i32) as usize);
            Task::none()
        }
        Message::Back => {
            if popup.current > 0 {
                popup.current -= 1;
                popup.cursor = None;
            }
            Task::none()
        }
        Message::Next => {
            let last = popup.current + 1 == popup.spec.questions.len();
            if last {
                let answers = (0..popup.spec.questions.len())
                    .map(|i| QuestionAnswer {
                        header: popup.spec.questions[i].header.clone(),
                        selections: popup.collect(i),
                    })
                    .collect();
                finish(Answer {
                    status: AnswerStatus::Answered,
                    answers,
                    error: None,
                });
            }
            popup.current += 1;
            popup.cursor = None;
            Task::none()
        }
        Message::Edit(action) => {
            popup.state_mut().note.perform(action);
            Task::none()
        }
        Message::Tick => match popup.remaining {
            Some(0) => finish(Answer {
                status: AnswerStatus::Timeout,
                answers: vec![],
                error: None,
            }),
            Some(secs) => {
                popup.remaining = Some(secs - 1);
                Task::none()
            }
            None => Task::none(),
        },
        Message::Close => finish(Answer {
            status: AnswerStatus::Cancelled,
            answers: vec![],
            error: None,
        }),
    }
}

// --------------------------------------------------------------------- view

/// The note editor displays up to 6 lines (the window reserves that space
/// statically — runtime window resizing proved unreliable: dropped resizes,
/// stale frames); beyond 6 lines it scrolls internally. Line height 20.0
/// measured from actual render (size 13 renders ~19.5px/line), + padding 16
/// + border 2, + 24px slack so a fresh caret line never triggers the
/// editor's internal scroll (its offset sticks and text renders over the
/// border).
/// Note editor: a full-size, fixed-height textarea (6 visible lines) — the
/// empty bottom part of the field is normal textarea space, not layout
/// leftover; past 6 lines it scrolls internally (the box never resizes, so
/// the internal scroll offset stays stable).
const EDITOR_H: f32 = 162.0;

fn view(popup: &Popup) -> Element<'_, Message> {
    let p = popup.palette;
    let n = popup.spec.questions.len();
    let q = popup.question();
    let st = popup.state();
    let last = popup.current + 1 == n;

    let mut col = column![].spacing(14).width(Length::Fill);

    if n > 1 {
        col = col.push(progress_bar(p, popup.current, n));
    }

    col = col.push(text(popup.topic_label()).size(11).color(p.accent));
    col = col.push(
        container(text(q.question.clone()).size(16).color(p.text))
            .width(Length::Fill)
            .padding([0, 1]),
    );

    let mut options = column![].spacing(2).width(Length::Fill);
    for (i, opt) in q.options.iter().enumerate() {
        let is_selected = st.selected[i];
        let is_cursor = popup.cursor == Some(i);
        if q.multi_select {
            options = options.push(multi_row(p, i, opt, is_selected, is_cursor));
        } else {
            options = options.push(single_row(p, i, opt, is_selected, is_cursor));
        }
    }
    col = col.push(
        container(scrollable(options))
            .width(Length::Fill)
            .height(Length::Shrink),
    );

    // Preview slot: always present when the question has previews — shows
    // either the code panel or the ↑/↓ tip, never changes size.
    if q.options.iter().any(|o| o.preview.is_some()) {
        let preview = popup
            .cursor
            .and_then(|c| q.options.get(c))
            .and_then(|o| o.preview.clone());
        col = col.push(preview_slot(p, preview));
    }

    // Multiline note editor: Enter inserts a newline (the editor consumes
    // the key), Shift+Enter sends — intercepted globally in `key_to_message`.
    col = col.push(
        text_editor(&st.note)
            .id(iced::widget::Id::new("aski-note"))
            .placeholder("Other… free-form answer")
            .size(13)
            .padding([8, 12])
            .width(CONTENT_W)
            .height(EDITOR_H)
            .style(move |_theme, status| editor_style(p, status))
            .on_action(Message::Edit),
    );

    col = col.push(nav_bar(p, popup.current > 0, last));

    if let Some(secs) = popup.remaining {
        col = col.push(
            container(
                text(format!("closes in {secs}s"))
                    .size(10)
                    .color(p.overlay0),
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
        );
    }

    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, PAD_X as u16])
        .style(move |_theme| container::Style {
            background: Some(p.base.into()),
            ..Default::default()
        })
        .into()
}

/// Thin progress line for multi-question runs: accent fill grows per question.
fn progress_bar(p: theme::Palette, current: usize, total: usize) -> Element<'static, Message> {
    let fraction = (current + 1) as f32 / total as f32;
    let fill_w = (CONTENT_W * fraction).max(12.0);
    container(
        container(text(""))
            .width(fill_w)
            .height(3)
            .style(move |_| container::Style {
                background: Some(p.accent.into()),
                border: Border {
                    radius: 2.0.into(),
                    width: 0.0,
                    color: p.accent,
                },
                ..Default::default()
            }),
    )
    .width(Length::Fill)
    .height(3)
    .style(move |_| container::Style {
        background: Some(p.surface0.into()),
        border: Border {
            radius: 2.0.into(),
            width: 0.0,
            color: p.surface0,
        },
        ..Default::default()
    })
    .into()
}

/// Round radio indicator drawn with pure geometry (no font glyph roulette).
fn radio_dot(p: theme::Palette, selected: bool) -> Element<'static, Message> {
    let inner = container(text(""))
        .width(5)
        .height(5)
        .style(move |_| container::Style {
            background: Some(if selected { p.base } else { p.mantle }.into()),
            ..Default::default()
        });
    container(inner)
        .width(14)
        .height(14)
        .center_x(14)
        .center_y(14)
        .style(move |_| container::Style {
            background: Some(if selected { p.accent } else { p.mantle }.into()),
            border: Border {
                radius: 7.0.into(),
                width: if selected { 0.0 } else { 1.0 },
                color: p.surface2,
            },
            ..Default::default()
        })
        .into()
}

/// Option texts: label + optional description. The label turns accent-colored
/// once the option is selected.
fn option_texts<'a>(
    p: theme::Palette,
    label: &'a str,
    description: Option<&'a String>,
    selected: bool,
) -> Element<'a, Message> {
    let mut col = column![].spacing(2);
    col =
        col.push(
            text(label.to_string())
                .size(15)
                .color(if selected { p.accent } else { p.text }),
        );
    if let Some(desc) = description {
        col = col.push(text(desc.clone()).size(12).color(p.overlay1));
    }
    col.into()
}

/// Row background: the keyboard cursor (and hover) get a subtle mantle plate;
/// uninteresting rows stay flat on the base color.
fn row_style(p: theme::Palette, hovered: bool, cursor: bool) -> (Option<iced::Background>, Border) {
    let bg = if hovered || cursor {
        Some(p.mantle.into())
    } else {
        None
    };
    (
        bg,
        Border {
            radius: 8.0.into(),
            width: 0.0,
            color: p.mantle,
        },
    )
}

fn single_row(
    p: theme::Palette,
    i: usize,
    opt: &crate::spec::PromptOption,
    selected: bool,
    cursor: bool,
) -> Element<'_, Message> {
    button(
        container(
            row![
                radio_dot(p, selected),
                option_texts(p, &opt.label, opt.description.as_ref(), selected)
            ]
            .spacing(12)
            .align_y(Vertical::Center),
        )
        .padding([9, 10])
        .width(Length::Fill),
    )
    .on_press(Message::Select(i))
    .width(Length::Fill)
    .style(move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (background, border) = row_style(p, hovered, cursor);
        button::Style {
            background,
            text_color: p.text,
            border,
            ..Default::default()
        }
    })
    .into()
}

fn multi_row(
    p: theme::Palette,
    i: usize,
    opt: &crate::spec::PromptOption,
    selected: bool,
    cursor: bool,
) -> Element<'_, Message> {
    // The whole row is one button (clicking the label must toggle too); the
    // checkbox is display-only, the button press flips the state.
    let cb = checkbox(selected)
        .label("")
        .size(16)
        .style(move |_theme, _status| checkbox::Style {
            background: if selected {
                p.accent.into()
            } else {
                p.mantle.into()
            },
            icon_color: p.base,
            border: Border {
                radius: 5.0.into(),
                width: if selected { 0.0 } else { 1.0 },
                color: p.surface2,
            },
            text_color: Some(p.text),
        });
    button(
        container(
            row![
                cb,
                option_texts(p, &opt.label, opt.description.as_ref(), selected)
            ]
            .spacing(12)
            .align_y(Vertical::Center),
        )
        .padding([9, 10])
        .width(Length::Fill),
    )
    .on_press(Message::Toggle(i, !selected))
    .width(Length::Fill)
    .style(move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (background, border) = row_style(p, hovered, cursor);
        button::Style {
            background,
            text_color: p.text,
            border,
            ..Default::default()
        }
    })
    .into()
}

/// Fixed-height preview slot, styled identically to the note editor's box
/// (mantle background, 8px radius, surface0 border) so it reads as part of
/// the form: shows the scrollable code preview for the cursor option, or the
/// centered ↑/↓ tip as a placeholder. Never changes size — the window is
/// static, so preview on/off must not shift the layout.
fn preview_slot(p: theme::Palette, preview: Option<String>) -> Element<'static, Message> {
    const SLOT_H: f32 = 70.0;
    let inner: Element<'static, Message> = match preview {
        Some(code) => column![
            text("PREVIEW").size(10).color(p.accent),
            scrollable(text(code).size(12).color(p.subtext0)).height(Length::Fill),
        ]
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        None => container(
            text("tip: press the up/down arrows to preview options")
                .size(11)
                .color(p.overlay0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Vertical::Center)
        .into(),
    };
    container(inner)
        .width(Length::Fill)
        .height(SLOT_H)
        .padding([8, 12])
        .style(move |_| container::Style {
            background: Some(p.mantle.into()),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: p.surface0,
            },
            ..Default::default()
        })
        .into()
}

/// Next/Submit, plus Back only when there is something to go back to.
fn nav_bar(p: theme::Palette, can_go_back: bool, last: bool) -> Element<'static, Message> {
    let nav_label = if last { "Submit" } else { "Next" };
    let next = button(
        container(text(nav_label.to_string()).size(13).color(p.crust))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([9, 0]),
    )
    .on_press(Message::Next)
    .width(Length::Fill)
    .style(move |_theme, status| primary_style(p, status));

    if !can_go_back {
        return row![next].width(Length::Fill).into();
    }

    row![
        button(
            container(text("Back").size(13).color(p.subtext0))
                .width(Length::Fill)
                .center_x(Length::Fill)
                .padding([9, 0]),
        )
        .on_press(Message::Back)
        .width(Length::Fill)
        .style(move |_theme, status| ghost_style(p, status)),
        next,
    ]
    .spacing(8)
    .width(Length::Fill)
    .align_y(Vertical::Center)
    .into()
}

// ------------------------------------------------------------------- styles

fn ghost_style(p: theme::Palette, status: button::Status) -> button::Style {
    let (background, border_color) = match status {
        button::Status::Hovered | button::Status::Pressed => (Some(p.surface0.into()), p.accent),
        _ => (None, p.surface1),
    };
    button::Style {
        background,
        text_color: p.subtext0,
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: border_color,
        },
        ..Default::default()
    }
}

fn primary_style(p: theme::Palette, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => p.subtext0,
        _ => p.accent,
    };
    button::Style {
        background: Some(background.into()),
        text_color: p.crust,
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: p.accent,
        },
        ..Default::default()
    }
}

fn editor_style(p: theme::Palette, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused { .. } => p.accent,
        _ => p.surface1,
    };
    text_editor::Style {
        background: p.mantle.into(),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: border_color,
        },
        placeholder: p.overlay0,
        value: p.text,
        selection: p.accent,
    }
}

// ------------------------------------------------------- events & window

fn subscription(popup: &Popup) -> Subscription<Message> {
    let mut subs = vec![
        window::close_requests().map(|_| Message::Close),
        iced::event::listen_with(|event, status, _id| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                key_to_message(key, modifiers, status)
            }
            _ => None,
        }),
    ];
    if popup.remaining.is_some() {
        subs.push(time::every(Duration::from_secs(1)).map(|_| Message::Tick));
    }
    Subscription::batch(subs)
}

// Character keys are never handled here — they must reach the note editor.
// `status` tells us whether a widget (the focused editor) already consumed
// the key: if it did, we stay out of the way (Enter = newline, arrows = caret).
// While typing, Shift+Enter is the explicit "send" — it fires regardless of
// focus, so the stray newline the editor inserts is discarded with the process.
fn key_to_message(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
    status: iced::event::Status,
) -> Option<Message> {
    use keyboard::key::Named;
    let ignored = matches!(status, iced::event::Status::Ignored);
    match key {
        keyboard::Key::Named(Named::Escape) => Some(Message::Close),
        keyboard::Key::Named(Named::Enter) if modifiers.shift() => Some(Message::Next),
        keyboard::Key::Named(Named::Enter) if ignored => Some(Message::Next),
        keyboard::Key::Named(Named::ArrowUp) if ignored => Some(Message::MoveCursor(-1)),
        keyboard::Key::Named(Named::ArrowDown) if ignored => Some(Message::MoveCursor(1)),
        _ => None,
    }
}

/// Estimated rendered height of a text block at the given font size: visual
/// lines from char counts (avg glyph ~0.55em on the 360px content box) ×
/// ~1.3 line height. Heuristic on purpose — deterministic, computed once at
/// startup, no runtime window resizing (that proved unreliable).
fn text_height(text: &str, size: f32) -> f32 {
    let chars_per_line = (CONTENT_W / (0.55 * size)).max(10.0);
    let lines: usize = text
        .lines()
        .map(|l| ((l.chars().count() as f32 / chars_per_line).ceil().max(1.0)) as usize)
        .sum();
    lines as f32 * size * 1.3
}

fn window_settings(spec: &PopupSpec) -> window::Settings {
    // The window is fully static (runtime resizing proved unreliable: dropped
    // resizes, stale frames), so it is sized for the TALLEST question with
    // content-aware wrapping estimates: long labels/descriptions wrap into
    // multiple lines and the flat per-row guess used to clip the nav row.
    // Editor is pre-reserved at 6 lines (162px); past that it scrolls.
    let mut height = 0.0f32;
    for q in &spec.questions {
        let mut h = 36.0 // vertical padding
            + 14.0 + 14.0 // topic label + spacing
            + text_height(&q.question, 16.0)
            + 14.0
            + EDITOR_H
            + 14.0 // note editor (6 lines) + spacing
            + 36.0
            + 14.0 // nav + spacing
            + 26.0; // countdown + its spacing
        for opt in &q.options {
            h += 20.0 // row padding
                + text_height(&opt.label, 15.0)
                + opt
                    .description
                    .as_ref()
                    .map(|d| text_height(d, 12.0))
                    .unwrap_or(0.0)
                + 2.0; // row spacing
        }
        h += 14.0; // spacing before the options block
        height = height.max(h);
    }
    if spec.questions.len() > 1 {
        height += 20.0; // progress bar
    }
    let has_preview = spec
        .questions
        .iter()
        .any(|q| q.options.iter().any(|o| o.preview.is_some()));
    // Preview slot ~70 + its spacing (always present when any option has a
    // preview: tip or code panel, fixed height).
    if has_preview {
        height += 84.0;
    }
    window::Settings {
        size: Size::new(WIN_W, height.clamp(300.0, 900.0)),
        // false on purpose: tiling compositors treat resizable windows as
        // tileable and blow the popup up to a full frame; non-resizable ones
        // stay floating. The content-aware estimate above is generous instead.
        resizable: false,
        decorations: false,
        level: window::Level::AlwaysOnTop,
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..Default::default()
    }
}
