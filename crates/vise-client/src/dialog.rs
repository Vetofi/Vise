use crate::text::Text;
use vise_world::nbt::Tag;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Dialog {
    pub kind: Kind,
    pub title: Text,
    pub external_title: Option<Text>,
    pub can_close_with_escape: bool,
    pub pause: bool,
    pub after_action: AfterAction,
    pub body: Vec<Body>,
    pub inputs: Vec<Input>,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Notice(Button),
    Confirmation {
        yes: Button,
        no: Button,
    },
    MultiAction {
        actions: Vec<Button>,
        exit: Option<Button>,
        columns: i32,
    },
    ServerLinks {
        exit: Option<Button>,
        columns: i32,
        button_width: i32,
    },
    DialogList {
        dialogs: Tag,
        entries: Vec<Button>,
        exit: Option<Button>,
        columns: i32,
        button_width: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterAction {
    Close,
    None,
    WaitForResponse,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub label: Text,
    pub tooltip: Option<Text>,
    pub width: i32,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Static(Click),
    RunCommand(String),
    Custom { id: String, additions: Option<Tag> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Click {
    OpenUrl(String),
    RunCommand(String),
    SuggestCommand(String),
    ShowDialog(Tag),
    ChangePage(i32),
    CopyToClipboard(String),
    Custom {
        id: String,
        payload: Option<Tag>,
    },
}

#[derive(Clone, Debug)]
pub enum Body {
    Message { contents: Text, width: i32 },
    Item {
        item: Tag,
        description: Option<(Text, i32)>,
        show_decorations: bool,
        show_tooltip: bool,
        width: i32,
        height: i32,
    },
}

#[derive(Clone, Debug)]
pub struct Input {
    pub key: String,
    pub control: Control,
}

#[derive(Clone, Debug)]
pub enum Control {
    Text {
        width: i32,
        label: Text,
        label_visible: bool,
        initial: String,
        max_length: i32,
        multiline: Option<(Option<i32>, Option<i32>)>,
    },
    Boolean {
        label: Text,
        initial: bool,
        on_true: String,
        on_false: String,
    },
    SingleOption {
        width: i32,
        options: Vec<(String, Option<Text>, bool)>,
        label: Text,
        label_visible: bool,
    },
    NumberRange {
        width: i32,
        label: Text,
        label_format: String,
        start: f32,
        end: f32,
        initial: Option<f32>,
        step: Option<f32>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Bool(bool),
    Option(usize),
    Slider(f32),
}

fn text(t: &Tag) -> Text {
    crate::scoreboard::comp_text(&crate::scoreboard::tag_to_json(t))
}
fn string(t: Option<&Tag>) -> Option<String> {
    match t? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}
fn number(t: Option<&Tag>) -> Option<f64> {
    Some(match t? {
        Tag::Byte(v) => f64::from(*v),
        Tag::Short(v) => f64::from(*v),
        Tag::Int(v) => f64::from(*v),
        Tag::Long(v) => *v as f64,
        Tag::Float(v) => f64::from(f32::from_bits(*v)),
        Tag::Double(v) => f64::from_bits(*v),
        _ => return None,
    })
}
fn int(t: &Tag, key: &str, default: i32) -> i32 {
    number(t.get(key)).map_or(default, |v| v as i32)
}
fn boolean(t: &Tag, key: &str, default: bool) -> bool {
    number(t.get(key)).map_or(default, |v| v != 0.0)
}
fn list(t: Option<&Tag>) -> &[Tag] {
    match t {
        Some(Tag::List(items)) => items,
        _ => &[],
    }
}
fn path(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

impl Button {
    fn parse(t: &Tag) -> Option<Self> {
        Some(Self {
            label: text(t.get("label")?),
            tooltip: t.get("tooltip").map(text),
            width: int(t, "width", 150),
            action: t.get("action").and_then(Action::parse),
        })
    }
    fn ok() -> Self {
        Self {
            label: Text::translate("gui.ok", &[]),
            tooltip: None,
            width: 150,
            action: None,
        }
    }
}

impl Click {
    pub fn parse(t: &Tag, kind: &str) -> Option<Self> {
        let s = |key| string(t.get(key));
        Some(match path(kind) {
            "open_url" => Self::OpenUrl(s("url")?),
            "run_command" => Self::RunCommand(s("command")?),
            "suggest_command" => Self::SuggestCommand(s("command")?),
            "show_dialog" => Self::ShowDialog(t.get("dialog")?.clone()),
            "change_page" => Self::ChangePage(int(t, "page", 1)),
            "copy_to_clipboard" => Self::CopyToClipboard(s("value")?),
            "custom" => Self::Custom {
                id: s("id")?,
                payload: t.get("payload").cloned(),
            },
            _ => return None,
        })
    }
}

impl Action {
    fn parse(t: &Tag) -> Option<Self> {
        let kind = string(t.get("type"))?;
        Some(match path(&kind) {
            "dynamic/run_command" => Self::RunCommand(string(t.get("template"))?),
            "dynamic/custom" => Self::Custom {
                id: string(t.get("id"))?,
                additions: t.get("additions").cloned(),
            },
            other => Self::Static(Click::parse(t, other)?),
        })
    }
    pub fn click(&self, inputs: &[Input], values: &[InputValue]) -> Option<Click> {
        let values = || inputs.iter().zip(values);
        Some(match self {
            Self::Static(click) => click.clone(),
            Self::RunCommand(template) => Click::RunCommand(substitute(template, &|key| {
                values()
                    .find(|(i, _)| i.key == key)
                    .map(|(i, v)| i.control.substitution(v))
            })),
            Self::Custom { id, additions } => {
                let mut tag = match additions {
                    Some(t @ Tag::Compound(_)) => t.clone(),
                    _ => Tag::Compound(Default::default()),
                };
                for (input, value) in values() {
                    if let Tag::Compound(map) = &mut tag {
                        map.insert(input.key.as_str().into(), input.control.tag(value));
                    }
                }
                Click::Custom {
                    id: id.clone(),
                    payload: Some(tag),
                }
            }
        })
    }
}

fn substitute(template: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(at) = rest.find("$(") {
        let Some(end) = rest[at..].find(')') else {
            break;
        };
        out.push_str(&rest[..at]);
        out.push_str(&value(&rest[at + 2..at + end]).unwrap_or_default());
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' | '\'' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl Control {
    fn parse(t: &Tag) -> Option<Self> {
        let label = || t.get("label").map(text);
        Some(match path(&string(t.get("type"))?) {
            "text" => {
                let initial = string(t.get("initial")).unwrap_or_default();
                let max_length = int(t, "max_length", 32);
                if initial.encode_utf16().count() > max_length.max(0) as usize {
                    return None;
                }
                Self::Text {
                    width: int(t, "width", 200),
                    label: label()?,
                    label_visible: boolean(t, "label_visible", true),
                    initial,
                    max_length,
                    multiline: t.get("multiline").map(|m| {
                        let get = |key| number(m.get(key)).map(|v| v as i32);
                        (get("max_lines"), get("height"))
                    }),
                }
            }
            "boolean" => Self::Boolean {
                label: label()?,
                initial: boolean(t, "initial", false),
                on_true: string(t.get("on_true")).unwrap_or_else(|| "true".into()),
                on_false: string(t.get("on_false")).unwrap_or_else(|| "false".into()),
            },
            "single_option" => {
                let options: Vec<_> = list(t.get("options"))
                    .iter()
                    .map(|o| match o {
                        Tag::String(id) => Some((id.to_string_lossy(), None, false)),
                        o => Some((
                            string(o.get("id"))?,
                            o.get("display").map(text),
                            boolean(o, "initial", false),
                        )),
                    })
                    .collect::<Option<_>>()?;
                if options.is_empty() || options.iter().filter(|o| o.2).count() > 1 {
                    return None;
                }
                Self::SingleOption {
                    width: int(t, "width", 200),
                    options,
                    label: label()?,
                    label_visible: boolean(t, "label_visible", true),
                }
            }
            "number_range" => {
                let float = |key| number(t.get(key)).map(|v| v as f32);
                Self::NumberRange {
                    width: int(t, "width", 200),
                    label: label()?,
                    label_format: string(t.get("label_format"))
                        .unwrap_or_else(|| "options.generic_value".into()),
                    start: float("start")?,
                    end: float("end")?,
                    initial: float("initial"),
                    step: float("step"),
                }
            }
            _ => return None,
        })
    }
    pub fn initial(&self) -> InputValue {
        match self {
            Self::Text { initial, .. } => InputValue::Text(initial.clone()),
            Self::Boolean { initial, .. } => InputValue::Bool(*initial),
            Self::SingleOption { options, .. } => {
                InputValue::Option(options.iter().position(|o| o.2).unwrap_or(0))
            }
            Self::NumberRange {
                start,
                end,
                initial,
                ..
            } => InputValue::Slider(to_slider(
                initial.unwrap_or((start + end) / 2.0),
                *start,
                *end,
            )),
        }
    }
    pub fn message(&self, value: &InputValue) -> Option<Text> {
        match (self, value) {
            (
                Self::SingleOption {
                    options,
                    label,
                    label_visible,
                    ..
                },
                InputValue::Option(i),
            ) => {
                let (id, display, _) = options.get(*i)?;
                let shown = display.clone().unwrap_or_else(|| Text::literal(id));
                Some(if *label_visible {
                    Text::translate("options.generic_value", &[label.clone(), shown])
                } else {
                    shown
                })
            }
            (
                Self::NumberRange {
                    label,
                    label_format,
                    ..
                },
                InputValue::Slider(_),
            ) => {
                let shown = number_string(self.scaled(value)?);
                Some(Text::translate(
                    label_format,
                    &[label.clone(), Text::literal(shown)],
                ))
            }
            _ => None,
        }
    }
    pub fn substitution(&self, value: &InputValue) -> String {
        match (self, value) {
            (Self::Text { .. }, InputValue::Text(s)) => escape(s),
            (
                Self::Boolean {
                    on_true, on_false, ..
                },
                InputValue::Bool(b),
            ) => if *b { on_true } else { on_false }.clone(),
            (Self::SingleOption { options, .. }, InputValue::Option(i)) => {
                options.get(*i).map(|o| o.0.clone()).unwrap_or_default()
            }
            (Self::NumberRange { .. }, InputValue::Slider(_)) => {
                number_string(self.scaled(value).unwrap_or(0.0))
            }
            _ => String::new(),
        }
    }
    pub fn tag(&self, value: &InputValue) -> Tag {
        match (self, value) {
            (Self::Boolean { .. }, InputValue::Bool(b)) => Tag::Byte(i8::from(*b)),
            (Self::NumberRange { .. }, _) => {
                Tag::Float(self.scaled(value).unwrap_or(0.0).to_bits())
            }
            (Self::Text { .. }, InputValue::Text(s)) => Tag::String(s.as_str().into()),
            _ => Tag::String(self.substitution(value).as_str().into()),
        }
    }
    pub fn scaled(&self, value: &InputValue) -> Option<f32> {
        let (
            Self::NumberRange {
                start,
                end,
                initial,
                step,
                ..
            },
            InputValue::Slider(slider),
        ) = (self, value)
        else {
            return None;
        };
        let (start, end) = (*start, *end);
        let in_range = start + slider * (end - start);
        let Some(step) = step else {
            return Some(in_range);
        };
        let first = initial.unwrap_or((start + end) / 2.0);
        let steps = ((in_range - first) / step + 0.5).floor() as i32;
        let result = first + steps as f32 * step;
        let position = to_slider(result, start, end);
        if (0.0..=1.0).contains(&position) {
            return Some(result);
        }
        Some(first + (steps - steps.signum()) as f32 * step)
    }
}

fn to_slider(value: f32, start: f32, end: f32) -> f32 {
    if start == end {
        0.5
    } else {
        (value - start) / (end - start)
    }
}

pub fn number_string(v: f32) -> String {
    if v == (v as i32) as f32 {
        (v as i32).to_string()
    } else {
        crate::snbt::java_float(v)
    }
}

impl Body {
    fn parse(t: &Tag) -> Option<Self> {
        let message = |t: &Tag| -> Option<(Text, i32)> {
            match t {
                Tag::Compound(_) if t.get("contents").is_some() => {
                    Some((text(t.get("contents")?), int(t, "width", 200)))
                }
                t => Some((text(t), 200)),
            }
        };
        Some(match path(&string(t.get("type"))?) {
            "plain_message" => {
                let (contents, width) = message(t)?;
                Self::Message { contents, width }
            }
            "item" => Self::Item {
                item: t.get("item")?.clone(),
                description: t.get("description").and_then(message),
                show_decorations: boolean(t, "show_decorations", true),
                show_tooltip: boolean(t, "show_tooltip", true),
                width: int(t, "width", 16),
                height: int(t, "height", 16),
            },
            _ => return None,
        })
    }
}

impl Dialog {
    pub fn parse(t: &Tag) -> Option<Self> {
        let button = |key: &str| t.get(key).and_then(Button::parse);
        let kind = match path(&string(t.get("type"))?) {
            "notice" => Kind::Notice(match t.get("action") {
                Some(b) => Button::parse(b)?,
                None => Button::ok(),
            }),
            "confirmation" => Kind::Confirmation {
                yes: button("yes")?,
                no: button("no")?,
            },
            "multi_action" => Kind::MultiAction {
                actions: list(t.get("actions"))
                    .iter()
                    .map(Button::parse)
                    .collect::<Option<Vec<_>>>()
                    .filter(|a| !a.is_empty())?,
                exit: button("exit_action"),
                columns: int(t, "columns", 2),
            },
            "server_links" => Kind::ServerLinks {
                exit: button("exit_action"),
                columns: int(t, "columns", 2),
                button_width: int(t, "button_width", 150),
            },
            "dialog_list" => Kind::DialogList {
                dialogs: t.get("dialogs")?.clone(),
                entries: Vec::new(),
                exit: button("exit_action"),
                columns: int(t, "columns", 2),
                button_width: int(t, "button_width", 150),
            },
            _ => return None,
        };
        let body = match t.get("body") {
            Some(Tag::List(items)) => items.iter().map(Body::parse).collect::<Option<_>>()?,
            Some(one) => vec![Body::parse(one)?],
            None => Vec::new(),
        };
        let inputs = list(t.get("inputs"))
            .iter()
            .map(|i| {
                Some(Input {
                    key: string(i.get("key"))?,
                    control: Control::parse(i)?,
                })
            })
            .collect::<Option<_>>()?;
        Some(Self {
            kind,
            title: text(t.get("title")?),
            external_title: t.get("external_title").map(text),
            can_close_with_escape: boolean(t, "can_close_with_escape", true),
            pause: boolean(t, "pause", true),
            after_action: match string(t.get("after_action")).as_deref() {
                Some("none") => AfterAction::None,
                Some("wait_for_response") => AfterAction::WaitForResponse,
                _ => AfterAction::Close,
            },
            body,
            inputs,
        })
    }
    pub fn on_cancel(&self) -> Option<&Action> {
        match &self.kind {
            Kind::Notice(b) => b.action.as_ref(),
            Kind::Confirmation { no, .. } => no.action.as_ref(),
            Kind::MultiAction { exit, .. }
            | Kind::ServerLinks { exit, .. }
            | Kind::DialogList { exit, .. } => exit.as_ref().and_then(|b| b.action.as_ref()),
        }
    }
    pub fn buttons(&self) -> Vec<&Button> {
        match &self.kind {
            Kind::Notice(b) => vec![b],
            Kind::Confirmation { yes, no } => vec![yes, no],
            Kind::MultiAction { actions, exit, .. } => actions.iter().chain(exit).collect(),
            Kind::ServerLinks { exit, .. } => exit.iter().collect(),
            Kind::DialogList { entries, exit, .. } => entries.iter().chain(exit).collect(),
        }
    }
    pub fn set_list(&mut self, dialogs: &[Tag]) {
        let Kind::DialogList {
            entries,
            button_width,
            ..
        } = &mut self.kind
        else {
            return;
        };
        *entries = dialogs
            .iter()
            .filter_map(|tag| {
                let sub = Dialog::parse(tag)?;
                Some(Button {
                    label: sub.external_title.unwrap_or(sub.title),
                    tooltip: None,
                    width: *button_width,
                    action: Some(Action::Static(Click::ShowDialog(tag.clone()))),
                })
            })
            .collect();
    }
    pub fn list_entries(
        dialogs: &Tag,
        tag: &dyn Fn(&str) -> Vec<Tag>,
        registry: &dyn Fn(&str) -> Option<Tag>,
    ) -> Vec<Tag> {
        let one = |t: &Tag| match t {
            Tag::String(id) => registry(&id.to_string_lossy()),
            inline => Some(inline.clone()),
        };
        match dialogs {
            Tag::String(id) if id.to_string_lossy().starts_with('#') => {
                tag(&id.to_string_lossy()[1..])
            }
            Tag::List(items) => items.iter().filter_map(one).collect(),
            single => one(single).into_iter().collect(),
        }
    }
    pub fn json(&self, values: &[InputValue]) -> Value {
        let kind = match &self.kind {
            Kind::Notice(_) => "notice",
            Kind::Confirmation { .. } => "confirmation",
            Kind::MultiAction { .. } => "multi_action",
            Kind::ServerLinks { .. } => "server_links",
            Kind::DialogList { .. } => "dialog_list",
        };
        let inputs: Vec<Value> = self
            .inputs
            .iter()
            .zip(values)
            .map(|(i, v)| {
                let kind = match i.control {
                    Control::Text { .. } => "text",
                    Control::Boolean { .. } => "boolean",
                    Control::SingleOption { .. } => "single_option",
                    Control::NumberRange { .. } => "number_range",
                };
                json!({"key": i.key, "type": kind, "value": i.control.substitution(v)})
            })
            .collect();
        json!({
            "type": kind,
            "title": self.title.plain(),
            "body": self.body.iter().map(|b| match b {
                Body::Message { contents, .. } => contents.plain(),
                Body::Item { item, .. } => string(item.get("id")).unwrap_or_default(),
            }).collect::<Vec<_>>(),
            "inputs": inputs,
            "buttons": self.buttons().iter().map(|b| b.label.plain()).collect::<Vec<_>>(),
            "can_close_with_escape": self.can_close_with_escape,
            "after_action": match self.after_action {
                AfterAction::Close => "close",
                AfterAction::None => "none",
                AfterAction::WaitForResponse => "wait_for_response",
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Tag {
        Tag::String(v.into())
    }

    fn register() -> Tag {
        Tag::compound([
            ("type", s("minecraft:multi_action")),
            ("title", s("Register")),
            ("can_close_with_escape", Tag::Byte(0)),
            ("after_action", s("wait_for_response")),
            (
                "body",
                Tag::compound([
                    ("type", s("minecraft:plain_message")),
                    ("contents", s("Choose a password")),
                ]),
            ),
            (
                "inputs",
                Tag::List(vec![Tag::compound([
                    ("type", s("minecraft:text")),
                    ("key", s("password")),
                    ("label", s("Password")),
                ])]),
            ),
            (
                "actions",
                Tag::List(vec![
                    Tag::compound([
                        ("label", s("Register")),
                        (
                            "action",
                            Tag::compound([
                                ("type", s("minecraft:dynamic/custom")),
                                ("id", s("authme:prejoin-register/submit")),
                            ]),
                        ),
                    ]),
                    Tag::compound([
                        ("label", s("Cancel")),
                        (
                            "action",
                            Tag::compound([
                                ("type", s("minecraft:dynamic/run_command")),
                                ("template", s("say $(password) $(missing)!")),
                            ]),
                        ),
                    ]),
                ]),
            ),
        ])
    }

    #[test]
    fn decodes_a_multi_action_dialog_with_defaults() {
        let d = Dialog::parse(&register()).unwrap();
        assert_eq!(d.title.plain(), "Register");
        assert!(!d.can_close_with_escape);
        assert!(d.pause);
        assert_eq!(d.after_action, AfterAction::WaitForResponse);
        assert!(matches!(&d.body[..], [Body::Message { width: 200, .. }]));
        let Kind::MultiAction {
            actions,
            exit,
            columns,
        } = &d.kind
        else {
            panic!("{:?}", d.kind)
        };
        assert_eq!((actions.len(), exit.is_none(), *columns), (2, true, 2));
        assert_eq!(actions[0].width, 150);
        assert!(d.on_cancel().is_none());
        let [
            Input {
                key,
                control:
                    Control::Text {
                        max_length: 32,
                        width: 200,
                        ..
                    },
            },
        ] = &d.inputs[..]
        else {
            panic!("{:?}", d.inputs)
        };
        assert_eq!(key, "password");
    }

    #[test]
    fn actions_carry_the_input_values() {
        let d = Dialog::parse(&register()).unwrap();
        let values = [InputValue::Text("p\"w".into())];
        let Kind::MultiAction { actions, .. } = &d.kind else {
            unreachable!()
        };
        let custom = actions[0]
            .action
            .as_ref()
            .unwrap()
            .click(&d.inputs, &values);
        assert_eq!(
            custom,
            Some(Click::Custom {
                id: "authme:prejoin-register/submit".into(),
                payload: Some(Tag::compound([("password", s("p\"w"))])),
            })
        );
        let command = actions[1]
            .action
            .as_ref()
            .unwrap()
            .click(&d.inputs, &values);
        assert_eq!(command, Some(Click::RunCommand("say p\\\"w !".into())));
    }

    #[test]
    fn a_notice_without_an_action_has_ok() {
        let d = Dialog::parse(&Tag::compound([("type", s("notice")), ("title", s("Hi"))])).unwrap();
        let Kind::Notice(b) = &d.kind else {
            unreachable!()
        };
        assert_eq!(b.label, Text::translate("gui.ok", &[]));
        assert!(Dialog::parse(&Tag::compound([("type", s("notice"))])).is_none());
        assert!(Dialog::parse(&Tag::compound([("type", s("x")), ("title", s("Hi"))])).is_none());
    }

    #[test]
    fn number_ranges_snap_to_their_step() {
        let mut range = Control::NumberRange {
            width: 200,
            label: Text::literal("n"),
            label_format: "options.generic_value".into(),
            start: 0.0,
            end: 10.0,
            initial: Some(4.0),
            step: Some(3.0),
        };
        assert_eq!(range.initial(), InputValue::Slider(0.4));
        assert_eq!(range.scaled(&InputValue::Slider(0.62)), Some(7.0));
        assert_eq!(range.scaled(&InputValue::Slider(1.0)), Some(10.0));
        let Control::NumberRange { step, .. } = &mut range else {
            unreachable!()
        };
        *step = Some(4.0);
        assert_eq!(range.scaled(&InputValue::Slider(1.0)), Some(8.0));
        assert_eq!(range.substitution(&InputValue::Slider(0.0)), "0");
        assert_eq!(number_string(2.5), "2.5");
    }
}
