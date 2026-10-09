//! The GUI popup: one process per tool call, one question on screen at a time,
//! borderless always-on-top window, lives exactly as long as the questions do.
//!
//! Keyboard: ↑/↓ move the option cursor, Enter commits the current question
//! (custom text > highlighted option > ticked options) and advances, Esc cancels.

use crate::spec::{Answer, AnswerStatus, PopupSpec, Question, QuestionAnswer};
use crate::theme;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{keyboard, time, window, Border, Element, Length, Size, Subscription, Task, Theme};
use std::io::Write;
use std::time::Duration;

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
    // wgpu's Vulkan backend freezes after the first frame on NVIDIA proprietary
    // drivers (events are processed but frames never present); GL renders fine.
    // wgpu reads WGPU_BACKEND at init, so set it before iced starts — but only
    // if the user hasn't chosen a backend themselves.
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "gl");
    }

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
    let input_id = text_input::Id::unique();
    iced::application("aski", update, view)
        .subscription(subscription)
        .theme(|_| Theme::Dark)
        .window(settings)
        .run_with(move || {
            let task = text_input::focus(input_id.clone());
            (Popup::new(spec, input_id), task)
        })
        .map_err(|e| format!("failed to run GUI: {e}"))
}

/// Per-question answer state.
struct QuestionState {
    selected: Vec<bool>,
    other: String,
}

struct Popup {
    spec: PopupSpec,
    palette: theme::Palette,
    states: Vec<QuestionState>,
    /// Index of the question currently on screen.
    current: usize,
    /// Keyboard-focused option within the current question.
    cursor: usize,
    input_id: text_input::Id,
    remaining: Option<u64>,
}

impl Popup {
    fn new(spec: PopupSpec, input_id: text_input::Id) -> Self {
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
                other: String::new(),
            })
            .collect();
        Self {
            spec,
            palette,
            states,
            current: 0,
            cursor: 0,
            input_id,
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

    /// Resolve the final selections for question `idx`: custom text wins, then
    /// ticked options, then (single-select only) the highlighted option.
    fn collect(&self, idx: usize) -> Vec<String> {
        let st = &self.states[idx];
        let q = &self.spec.questions[idx];
        let custom = st.other.trim();
        if !custom.is_empty() {
            return vec![custom.to_string()];
        }
        let ticked: Vec<String> = st
            .selected
            .iter()
            .enumerate()
            .filter(|(_, checked)| **checked)
            .map(|(i, _)| q.options[i].label.clone())
            .collect();
        if !ticked.is_empty() {
            return ticked;
        }
        if !q.multi_select {
            if let Some(opt) = q.options.get(self.cursor) {
                return vec![opt.label.clone()];
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
    OtherChanged(String),
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
            // Single-select: clicking highlights the option; Next/Enter commits.
            if popup.question().options.get(i).is_some() {
                for (j, slot) in popup.state_mut().selected.iter_mut().enumerate() {
                    *slot = j == i;
                }
                popup.cursor = i;
            }
            Task::none()
        }
        Message::Toggle(i, value) => {
            if let Some(slot) = popup.state_mut().selected.get_mut(i) {
                *slot = value;
            }
            popup.cursor = i;
            Task::none()
        }
        Message::MoveCursor(delta) => {
            let max = popup.question().options.len().saturating_sub(1);
            popup.cursor = (popup.cursor as i32 + delta).clamp(0, max as i32) as usize;
            Task::none()
        }
        Message::Back => {
            if popup.current > 0 {
                popup.current -= 1;
                popup.cursor = 0;
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
            popup.cursor = 0;
            Task::none()
        }
        Message::OtherChanged(value) => {
            popup.state_mut().other = value;
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

fn view(popup: &Popup) -> Element<'_, Message> {
    let p = popup.palette;
    let n = popup.spec.questions.len();
    let q = popup.question();
    let st = popup.state();
    let last = popup.current + 1 == n;
    let mut col = column![].spacing(12).width(Length::Fill);

    // Top bar: topic chips when there are several questions, plain header otherwise.
    if n > 1 {
        let mut tabs = row![].spacing(6);
        for (i, qq) in popup.spec.questions.iter().enumerate() {
            let label = qq
                .header
                .clone()
                .unwrap_or_else(|| format!("q{}", i + 1))
                .to_uppercase();
            let is_cur = i == popup.current;
            tabs = tabs.push(
                container(
                    text(label)
                        .size(11)
                        .color(if is_cur { p.accent } else { p.overlay0 }),
                )
                .padding(4)
                .style(move |_| container::Style {
                    background: Some(if is_cur { p.surface0 } else { p.mantle }.into()),
                    border: Border {
                        radius: 5.0.into(),
                        width: 1.0,
                        color: if is_cur { p.accent } else { p.surface1 },
                    },
                    ..Default::default()
                }),
            );
        }
        col = col.push(tabs);
    } else {
        col = col.push(
            text(
                q.header
                    .as_deref()
                    .unwrap_or("Decision needed")
                    .to_uppercase(),
            )
            .size(12)
            .color(p.accent),
        );
    }

    col = col.push(
        container(text(q.question.clone()).size(17).color(p.text))
            .width(Length::Fill)
            .padding(4),
    );

    let mut options = column![].spacing(8);
    for (i, opt) in q.options.iter().enumerate() {
        let is_selected = st.selected[i];
        let is_cursor = popup.cursor == i;
        if q.multi_select {
            let mut opt_col = column![].spacing(2);
            opt_col = opt_col.push(
                checkbox(opt.label.clone(), is_selected)
                    .on_toggle(move |checked| Message::Toggle(i, checked))
                    .size(18)
                    .text_size(14)
                    .style(move |_theme, _status| checkbox::Style {
                        background: p.surface0.into(),
                        icon_color: p.accent,
                        border: Border {
                            radius: 5.0.into(),
                            width: 1.0,
                            color: p.surface2,
                        },
                        text_color: Some(p.text),
                    }),
            );
            if let Some(desc) = &opt.description {
                opt_col = opt_col.push(
                    text(desc.clone())
                        .size(12)
                        .color(p.overlay1)
                        .width(Length::Fill),
                );
            }
            options = options.push(container(opt_col).width(Length::Fill).padding(6).style(
                move |_| container::Style {
                    background: Some(if is_selected { p.surface0 } else { p.mantle }.into()),
                    border: Border {
                        radius: 6.0.into(),
                        width: if is_cursor { 1.0 } else { 0.0 },
                        color: p.accent,
                    },
                    ..Default::default()
                },
            ));
        } else {
            let mut label_col = column![].spacing(2);
            label_col = label_col.push(text(opt.label.clone()).size(14).color(p.text));
            if let Some(desc) = &opt.description {
                label_col = label_col.push(text(desc.clone()).size(12).color(p.overlay1));
            }
            options = options.push(
                button(container(label_col).padding(8).width(Length::Fill))
                    .on_press(Message::Select(i))
                    .width(Length::Fill)
                    .style(move |_theme, status| option_style(p, status, is_selected, is_cursor)),
            );
        }
    }
    col = col.push(scrollable(options).height(Length::Shrink));

    // Preview of the focused option (code sample, mockup, diff, …).
    if let Some(preview) = q.options.get(popup.cursor).and_then(|o| o.preview.clone()) {
        col = col.push(
            container(scrollable(text(preview).size(12).color(p.subtext0)).height(Length::Fill))
                .width(Length::Fill)
                .height(150)
                .padding(8)
                .style(move |_| container::Style {
                    background: Some(p.mantle.into()),
                    border: Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: p.surface1,
                    },
                    ..Default::default()
                }),
        );
    }

    col = col.push(
        text_input("Other… (free-form answer)", &st.other)
            .id(popup.input_id.clone())
            .on_input(Message::OtherChanged)
            .on_submit(Message::Next)
            .size(13)
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme, status| text_input_style(p, status)),
    );

    let nav = row![
        button(text("Back").size(13).color(p.subtext0))
            .on_press_maybe((popup.current > 0).then_some(Message::Back))
            .padding([8, 14])
            .style(move |_theme, status| option_style(p, status, false, false)),
        button(
            container(
                text(if last { "Submit" } else { "Next" })
                    .size(14)
                    .color(p.crust)
            )
            .padding(8)
            .width(Length::Fill)
            .center_x(Length::Fill)
        )
        .on_press(Message::Next)
        .width(Length::Fill)
        .style(move |_theme, status| confirm_style(p, status)),
    ]
    .spacing(8);
    col = col.push(nav);

    if let Some(secs) = popup.remaining {
        col = col.push(
            container(
                text(format!("closes in {secs}s"))
                    .size(11)
                    .color(p.overlay1),
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
        );
    }

    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18)
        .style(move |_theme| container::Style {
            background: Some(p.base.into()),
            ..Default::default()
        })
        .into()
}

fn option_style(
    p: theme::Palette,
    status: button::Status,
    selected: bool,
    cursor: bool,
) -> button::Style {
    let highlight = selected || cursor;
    let (background, border_color, border_width) = match (highlight, status) {
        (true, _) => (p.surface1, p.accent, 1.0),
        (false, button::Status::Hovered | button::Status::Pressed) => (p.surface1, p.accent, 1.0),
        (false, _) => (p.surface0, p.surface2, 1.0),
    };
    button::Style {
        background: Some(background.into()),
        text_color: p.text,
        border: Border {
            radius: 8.0.into(),
            width: border_width,
            color: border_color,
        },
        ..Default::default()
    }
}

fn confirm_style(p: theme::Palette, status: button::Status) -> button::Style {
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

fn text_input_style(p: theme::Palette, status: text_input::Status) -> text_input::Style {
    let border_color = if matches!(status, text_input::Status::Focused) {
        p.accent
    } else {
        p.surface2
    };
    text_input::Style {
        background: p.mantle.into(),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: border_color,
        },
        icon: p.overlay0,
        placeholder: p.overlay0,
        value: p.text,
        selection: p.accent,
    }
}

fn subscription(popup: &Popup) -> Subscription<Message> {
    let mut subs = vec![
        window::close_requests().map(|_| Message::Close),
        keyboard::on_key_press(|key, _modifiers| key_to_message(key)),
    ];
    if popup.remaining.is_some() {
        subs.push(time::every(Duration::from_secs(1)).map(|_| Message::Tick));
    }
    Subscription::batch(subs)
}

// Character keys are deliberately NOT handled here: they must reach the
// "Other…" text input untouched. Up/Down are no-ops in a single-line input,
// so they are safe to repurpose as option-cursor navigation.
fn key_to_message(key: keyboard::Key) -> Option<Message> {
    use keyboard::key::Named;
    match key {
        keyboard::Key::Named(Named::Escape) => Some(Message::Close),
        keyboard::Key::Named(Named::Enter) => Some(Message::Next),
        keyboard::Key::Named(Named::ArrowUp) => Some(Message::MoveCursor(-1)),
        keyboard::Key::Named(Named::ArrowDown) => Some(Message::MoveCursor(1)),
        _ => None,
    }
}

fn window_settings(spec: &PopupSpec) -> window::Settings {
    let max_options = spec
        .questions
        .iter()
        .map(|q| q.options.len())
        .max()
        .unwrap_or(2) as f32;
    let has_preview = spec
        .questions
        .iter()
        .any(|q| q.options.iter().any(|o| o.preview.is_some()));
    let mut height = 240.0 + 66.0 * max_options;
    if spec.questions.len() > 1 {
        height += 40.0; // question tabs
    }
    if has_preview {
        height += 160.0; // preview panel
    }
    height += 60.0; // Back/Next row
    window::Settings {
        size: Size::new(460.0, height.clamp(320.0, 900.0)),
        resizable: false,
        decorations: false,
        level: window::Level::AlwaysOnTop,
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..Default::default()
    }
}
