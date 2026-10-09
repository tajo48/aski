//! The GUI popup: one question per process, borderless always-on-top window,
//! lives exactly as long as the question does.

use crate::spec::{Answer, AnswerStatus, QuestionSpec};
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
                selections: vec![],
                error: Some(e),
            });
            std::process::exit(1);
        }
    }
}

fn try_run_popup() -> Result<(), String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("failed to read question from stdin: {e}"))?;
    let spec: QuestionSpec = serde_json::from_str(line.trim())
        .map_err(|e| format!("invalid question JSON on stdin: {e}"))?;

    let settings = window_settings(&spec);
    iced::application("aski", update, view)
        .subscription(subscription)
        .theme(|_| Theme::Dark)
        .window(settings)
        .run_with(move || (Popup::new(spec), Task::none()))
        .map_err(|e| format!("failed to run GUI: {e}"))
}

struct Popup {
    spec: QuestionSpec,
    palette: theme::Palette,
    selected: Vec<bool>,
    other: String,
    remaining: Option<u64>,
    done: bool,
}

impl Popup {
    fn new(spec: QuestionSpec) -> Self {
        let flavor = spec.flavor.unwrap_or_default();
        let palette = theme::resolve(flavor, spec.accent.as_deref());
        let remaining = match spec.timeout_secs {
            Some(0) | None => None,
            Some(secs) => Some(secs),
        };
        Self {
            selected: vec![false; spec.options.len()],
            spec,
            palette,
            other: String::new(),
            remaining,
            done: false,
        }
    }

    fn header_label(&self) -> String {
        self.spec
            .header
            .as_deref()
            .unwrap_or("Decision needed")
            .to_uppercase()
    }
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Toggle(usize, bool),
    OtherChanged(String),
    Confirm,
    Digit(usize),
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
            if let Some(opt) = popup.spec.options.get(i) {
                finish(Answer {
                    status: AnswerStatus::Answered,
                    selections: vec![opt.label.clone()],
                    error: None,
                });
            }
            #[allow(unreachable_code)]
            Task::none()
        }
        Message::Toggle(i, value) => {
            if let Some(slot) = popup.selected.get_mut(i) {
                *slot = value;
            }
            Task::none()
        }
        Message::OtherChanged(value) => {
            popup.other = value;
            Task::none()
        }
        Message::Confirm => {
            let custom = popup.other.trim();
            let selections: Vec<String> = if !custom.is_empty() {
                vec![custom.to_string()]
            } else {
                popup
                    .selected
                    .iter()
                    .enumerate()
                    .filter(|(_, checked)| **checked)
                    .map(|(i, _)| popup.spec.options[i].label.clone())
                    .collect()
            };
            finish(Answer {
                status: AnswerStatus::Answered,
                selections,
                error: None,
            });
        }
        Message::Digit(i) => {
            if !popup.spec.multi_select && i < popup.spec.options.len() {
                let label = popup.spec.options[i].label.clone();
                finish(Answer {
                    status: AnswerStatus::Answered,
                    selections: vec![label],
                    error: None,
                });
            }
            #[allow(unreachable_code)]
            Task::none()
        }
        Message::Tick => match popup.remaining {
            Some(0) => finish(Answer {
                status: AnswerStatus::Timeout,
                selections: vec![],
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
            selections: vec![],
            error: None,
        }),
    }
}

fn view(popup: &Popup) -> Element<'_, Message> {
    let p = popup.palette;
    let mut col = column![].spacing(14).width(Length::Fill);

    col = col.push(text(popup.header_label()).size(12).color(p.accent));
    col = col.push(
        container(text(popup.spec.question.clone()).size(17).color(p.text))
            .width(Length::Fill)
            .padding(4),
    );

    let mut options = column![].spacing(8);
    if popup.spec.multi_select {
        for (i, opt) in popup.spec.options.iter().enumerate() {
            options = options.push(
                checkbox(opt.label.clone(), popup.selected[i])
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
                options = options.push(
                    text(desc.clone())
                        .size(12)
                        .color(p.overlay1)
                        .width(Length::Fill),
                );
            }
        }
        col = col.push(options);
        col = col.push(
            button(
                container(text("Confirm").size(14).color(p.crust))
                    .padding(8)
                    .width(Length::Fill)
                    .center_x(Length::Fill),
            )
            .on_press(Message::Confirm)
            .style(move |_theme, status| confirm_style(p, status)),
        );
    } else {
        for (i, opt) in popup.spec.options.iter().enumerate() {
            let mut label_col = column![].spacing(2);
            label_col = label_col.push(text(opt.label.clone()).size(14).color(p.text));
            if let Some(desc) = &opt.description {
                label_col = label_col.push(text(desc.clone()).size(12).color(p.overlay1));
            }
            options = options.push(
                button(container(label_col).padding(8).width(Length::Fill))
                    .on_press(Message::Select(i))
                    .width(Length::Fill)
                    .style(move |_theme, status| option_style(p, status)),
            );
        }
        col = col.push(scrollable(options).height(Length::Shrink));
    }

    col = col.push(
        row![text_input("Other… (free-form answer)", &popup.other)
            .on_input(Message::OtherChanged)
            .on_submit(Message::Confirm)
            .size(13)
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme, status| text_input_style(p, status)),]
        .spacing(8),
    );

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

fn option_style(p: theme::Palette, status: button::Status) -> button::Style {
    let (background, border_color) = match status {
        button::Status::Hovered | button::Status::Pressed => (p.surface1, p.accent),
        _ => (p.surface0, p.surface2),
    };
    button::Style {
        background: Some(background.into()),
        text_color: p.text,
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
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
    if popup.done {
        return Subscription::none();
    }
    let mut subs = vec![
        window::close_requests().map(|_| Message::Close),
        keyboard::on_key_press(|key, _modifiers| key_to_message(key)),
    ];
    if popup.remaining.is_some() {
        subs.push(time::every(Duration::from_secs(1)).map(|_| Message::Tick));
    }
    Subscription::batch(subs)
}

fn key_to_message(key: keyboard::Key) -> Option<Message> {
    use keyboard::key::Named;
    match key {
        keyboard::Key::Named(Named::Escape) => Some(Message::Close),
        keyboard::Key::Named(Named::Enter) => Some(Message::Confirm),
        keyboard::Key::Character(s) => s
            .chars()
            .next()
            .and_then(|ch| ch.to_digit(10))
            .map(|d| Message::Digit((d as usize).wrapping_sub(1))),
        _ => None,
    }
}

fn window_settings(spec: &QuestionSpec) -> window::Settings {
    let n = spec.options.len() as f32;
    let mut height = 230.0 + 76.0 * n;
    if spec.multi_select {
        height += 90.0;
    }
    window::Settings {
        size: Size::new(460.0, height.clamp(260.0, 800.0)),
        resizable: false,
        decorations: false,
        level: window::Level::AlwaysOnTop,
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..Default::default()
    }
}
