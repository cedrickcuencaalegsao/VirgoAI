use iced::alignment::Vertical;
use iced::widget::{
    button, column, horizontal_space, row, scrollable, text, text_editor,
};
use iced::{Element, Length, Task, Theme};

use crate::agent::pipeline::{self, Pending};
use crate::agent::{ChatMessage, Role};
use crate::config::Config;
use crate::memory::retrieval;
use crate::memory::store::{self, MemoryStore};
use crate::ollama::client;
use crate::ollama::types::StreamEvent;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Chat,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Planning,
    Executing,
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),

    // Chat
    InputChanged(String),
    Send,
    Stop,
    PlanDone(Result<String, String>),
    Stream(StreamEvent),
    PlannerSelected(String),
    ExecutorSelected(String),
    RefreshModels,
    ModelsLoaded(Result<Vec<String>, String>),
    RememberLast,

    // Memory sidebar
    ReloadNotes,
    SelectNote(String),
    EditorAction(text_editor::Action),
    SaveNote,
    DeleteNote,
    NewNoteNameChanged(String),
    CreateNote,

    // Settings
    SettingsUrlChanged(String),
    SettingsMemoryDirChanged(String),
    SettingsNumCtxChanged(String),
    SettingsKeepAliveChanged(String),
    ToggleUsePlanner(bool),
    TogglePlannerThink(bool),
    SaveSettings,
    WarmUp,
    WarmedUp(Result<String, String>),
}

pub struct App {
    pub cfg: Config,
    pub num_ctx_input: String,
    pub store: MemoryStore,

    pub screen: Screen,
    pub phase: Phase,
    pub status: String,

    pub models: Vec<String>,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub pending: Option<Pending>,
    pub abort: Option<iced::task::Handle>,

    pub notes: Vec<String>,
    pub selected_note: Option<String>,
    pub editor: text_editor::Content,
    pub new_note_name: String,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut cfg = Config::load();
        cfg.ollama_url = client::normalize_base(&cfg.ollama_url);
        let store = MemoryStore::new(cfg.memory_path());
        let _ = store.ensure();
        let notes = store.list();
        let num_ctx_input = cfg.num_ctx.to_string();
        let url = cfg.ollama_url.clone();

        let app = App {
            cfg,
            num_ctx_input,
            store,
            screen: Screen::Chat,
            phase: Phase::Idle,
            status: "Ready".into(),
            models: Vec::new(),
            messages: Vec::new(),
            input: String::new(),
            pending: None,
            abort: None,
            notes,
            selected_note: None,
            editor: text_editor::Content::new(),
            new_note_name: String::new(),
        };

        let task = Task::perform(client::list_models(url), Message::ModelsLoaded);
        (app, task)
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn is_busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub fn has_reply(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.role == Role::Assistant && !m.content.trim().is_empty())
    }

    // -----------------------------------------------------------------------
    // update
    // -----------------------------------------------------------------------

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => {
                self.screen = screen;
                Task::none()
            }

            // ---- chat ----
            Message::InputChanged(value) => {
                self.input = value;
                Task::none()
            }
            Message::Send => self.send(),
            Message::Stop => self.stop(),
            Message::PlanDone(result) => self.on_plan_done(result),
            Message::Stream(event) => self.on_stream(event),
            Message::PlannerSelected(model) => {
                self.cfg.planner_model = model;
                self.persist();
                Task::none()
            }
            Message::ExecutorSelected(model) => {
                self.cfg.executor_model = model;
                self.persist();
                Task::none()
            }
            Message::RefreshModels => self.refresh_models(),
            Message::ModelsLoaded(Ok(list)) => {
                self.status = format!("{} model(s) available", list.len());
                self.models = list;
                Task::none()
            }
            Message::ModelsLoaded(Err(error)) => {
                self.status = error;
                Task::none()
            }
            Message::RememberLast => {
                let reply = self
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant && !m.content.trim().is_empty())
                    .map(|m| m.content.trim().to_string());
                if let Some(reply) = reply {
                    let entry = format!("\n## {}\n{}\n", store::today(), reply);
                    self.status = match self.store.append("inbox.md", &entry) {
                        Ok(_) => "Saved the last reply to inbox.md".into(),
                        Err(e) => format!("Could not save: {e}"),
                    };
                    self.refresh_notes();
                }
                Task::none()
            }

            // ---- memory ----
            Message::ReloadNotes => {
                self.refresh_notes();
                Task::none()
            }
            Message::SelectNote(name) => {
                self.select_note(name);
                Task::none()
            }
            Message::EditorAction(action) => {
                self.editor.perform(action);
                Task::none()
            }
            Message::SaveNote => {
                if let Some(name) = self.selected_note.clone() {
                    self.status = match self.store.write(&name, &self.editor.text()) {
                        Ok(_) => format!("Saved {name}"),
                        Err(e) => format!("Could not save {name}: {e}"),
                    };
                }
                Task::none()
            }
            Message::DeleteNote => {
                if let Some(name) = self.selected_note.clone() {
                    self.status = match self.store.delete(&name) {
                        Ok(()) => format!("Deleted {name}"),
                        Err(e) => format!("Could not delete {name}: {e}"),
                    };
                    self.selected_note = None;
                    self.editor = text_editor::Content::new();
                    self.refresh_notes();
                }
                Task::none()
            }
            Message::NewNoteNameChanged(value) => {
                self.new_note_name = value;
                Task::none()
            }
            Message::CreateNote => {
                match store::normalize_name(&self.new_note_name) {
                    None => self.status = "Enter a valid note name".into(),
                    Some(name) => {
                        if !self.notes.contains(&name) {
                            if let Err(e) = self.store.write(&name, "") {
                                self.status = format!("Could not create {name}: {e}");
                                return Task::none();
                            }
                        }
                        self.new_note_name.clear();
                        self.refresh_notes();
                        self.select_note(name);
                    }
                }
                Task::none()
            }

            // ---- settings ----
            Message::SettingsUrlChanged(value) => {
                self.cfg.ollama_url = value;
                Task::none()
            }
            Message::SettingsMemoryDirChanged(value) => {
                self.cfg.memory_dir = value;
                Task::none()
            }
            Message::SettingsNumCtxChanged(value) => {
                self.num_ctx_input = value;
                Task::none()
            }
            Message::SettingsKeepAliveChanged(value) => {
                self.cfg.keep_alive = value;
                Task::none()
            }
            Message::ToggleUsePlanner(value) => {
                self.cfg.use_planner = value;
                Task::none()
            }
            Message::TogglePlannerThink(value) => {
                self.cfg.planner_think = value;
                Task::none()
            }
            Message::SaveSettings => self.save_settings(),
            Message::WarmUp => {
                let mut models = Vec::new();
                if self.cfg.use_planner {
                    models.push(self.cfg.planner_model.clone());
                }
                if !models.contains(&self.cfg.executor_model) {
                    models.push(self.cfg.executor_model.clone());
                }
                self.status = format!("Loading {}…", models.join(", "));
                Task::perform(
                    client::warm_up(
                        self.cfg.ollama_url.clone(),
                        models,
                        self.cfg.keep_alive.clone(),
                    ),
                    Message::WarmedUp,
                )
            }
            Message::WarmedUp(result) => {
                self.status = match result {
                    Ok(summary) => summary,
                    Err(error) => error,
                };
                Task::none()
            }
        }
    }

    // -----------------------------------------------------------------------
    // chat pipeline
    // -----------------------------------------------------------------------

    fn send(&mut self) -> Task<Message> {
        let text = self.input.trim().to_string();
        if text.is_empty() || self.is_busy() {
            return Task::none();
        }
        self.input.clear();

        // Gather context BEFORE adding the new message to the history.
        let history = pipeline::history_from(&self.messages);
        let notes = self.store.load_all();
        let budget_chars = self.cfg.num_ctx as usize * 4 / 3;
        let memory = retrieval::build_context(&notes, &text, budget_chars);

        self.messages.push(ChatMessage::new(Role::User, text.clone()));
        let pending = Pending { task: text, memory, history };

        if self.cfg.use_planner {
            let request = pipeline::planner_request(&self.cfg, &pending);
            self.pending = Some(pending);
            self.phase = Phase::Planning;
            self.status = format!("Planning with {}…", self.cfg.planner_model);

            let (task, handle) = Task::perform(
                client::chat_once(self.cfg.ollama_url.clone(), request),
                Message::PlanDone,
            )
            .abortable();
            self.abort = Some(handle);
            Task::batch([task, scroll_to_end()])
        } else {
            self.pending = Some(pending);
            self.start_executor(None)
        }
    }

    fn on_plan_done(&mut self, result: Result<String, String>) -> Task<Message> {
        self.abort = None;
        match result {
            Ok(raw) => {
                let plan = pipeline::strip_think_tags(&raw);
                if plan.is_empty() {
                    self.start_executor(None)
                } else {
                    self.messages.push(ChatMessage::new(Role::Plan, plan.clone()));
                    self.start_executor(Some(plan))
                }
            }
            Err(error) => self.fail(format!("Planner failed: {error}")),
        }
    }

    fn start_executor(&mut self, plan: Option<String>) -> Task<Message> {
        let Some(pending) = self.pending.take() else {
            return Task::none();
        };

        let request = pipeline::executor_request(&self.cfg, &pending, plan.as_deref());
        self.messages.push(ChatMessage::new(Role::Assistant, String::new()));
        self.phase = Phase::Executing;
        self.status = format!("Answering with {}…", self.cfg.executor_model);

        let stream = client::chat_stream(self.cfg.ollama_url.clone(), request);
        let (task, handle) = Task::run(stream, Message::Stream).abortable();
        self.abort = Some(handle);
        Task::batch([task, scroll_to_end()])
    }

    fn on_stream(&mut self, event: StreamEvent) -> Task<Message> {
        match event {
            StreamEvent::Token(token) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.content.push_str(&token);
                    }
                }
                scroll_to_end()
            }
            StreamEvent::Done => {
                self.phase = Phase::Idle;
                self.abort = None;
                self.status = "Done".into();
                Task::none()
            }
            StreamEvent::Error(error) => self.fail(error),
        }
    }

    fn stop(&mut self) -> Task<Message> {
        if let Some(handle) = self.abort.take() {
            handle.abort();
        }
        self.phase = Phase::Idle;
        self.pending = None;
        self.status = "Stopped".into();
        Task::none()
    }

    fn fail(&mut self, error: String) -> Task<Message> {
        // Drop an empty assistant bubble left behind by a failed stream.
        let empty_reply = matches!(
            self.messages.last(),
            Some(m) if m.role == Role::Assistant && m.content.is_empty()
        );
        if empty_reply {
            self.messages.pop();
        }

        self.messages.push(ChatMessage::new(Role::Error, error.clone()));
        self.phase = Phase::Idle;
        self.pending = None;
        self.abort = None;
        self.status = error;
        scroll_to_end()
    }

    fn refresh_models(&mut self) -> Task<Message> {
        self.status = "Asking Ollama for installed models…".into();
        Task::perform(
            client::list_models(self.cfg.ollama_url.clone()),
            Message::ModelsLoaded,
        )
    }

    // -----------------------------------------------------------------------
    // helpers
    // -----------------------------------------------------------------------

    fn persist(&mut self) {
        if let Err(e) = self.cfg.save() {
            self.status = format!("Could not save config: {e}");
        }
    }

    fn save_settings(&mut self) -> Task<Message> {
        match self.num_ctx_input.trim().parse::<u32>() {
            Ok(n) if n >= 512 => self.cfg.num_ctx = n,
            _ => {
                self.status = "Context size must be a number of at least 512".into();
                return Task::none();
            }
        }

        self.cfg.ollama_url = client::normalize_base(&self.cfg.ollama_url);

        // The memory folder may have changed: point the store at it.
        self.store = MemoryStore::new(self.cfg.memory_path());
        if let Err(e) = self.store.ensure() {
            self.status = format!("Could not use memory folder: {e}");
            return Task::none();
        }
        self.selected_note = None;
        self.editor = text_editor::Content::new();
        self.refresh_notes();

        match self.cfg.save() {
            Ok(()) => self.status = "Settings saved".into(),
            Err(e) => self.status = format!("Could not save config: {e}"),
        }
        self.refresh_models()
    }

    fn refresh_notes(&mut self) {
        self.notes = self.store.list();
        let missing = self
            .selected_note
            .as_ref()
            .is_some_and(|name| !self.notes.contains(name));
        if missing {
            self.selected_note = None;
            self.editor = text_editor::Content::new();
        }
    }

    fn select_note(&mut self, name: String) {
        let result: std::io::Result<String> = self.store.read(&name);
        match result {
            Ok(content) => {
                let body: &str = content.as_str();
                self.editor = text_editor::Content::with_text(body);
                self.selected_note = Some(name);
            }
            Err(e) => self.status = format!("Could not open {name}: {e}"),
        }
    }

    // -----------------------------------------------------------------------
    // view
    // -----------------------------------------------------------------------

    pub fn view(&self) -> Element<'_, Message> {
        let nav = row![
            text("Virgo AI").size(22),
            horizontal_space(),
            nav_button("Chat", Screen::Chat, self.screen),
            nav_button("Settings", Screen::Settings, self.screen),
        ]
        .spacing(8)
        .align_y(Vertical::Center);

        let body: Element<'_, Message> = match self.screen {
            Screen::Chat => row![ui::sidebar::view(self), ui::chat::view(self)]
                .spacing(10)
                .height(Length::Fill)
                .into(),
            Screen::Settings => ui::settings::view(self),
        };

        let status = text(self.status.as_str()).size(12);

        column![nav, body, status].spacing(10).padding(12).into()
    }
}

fn scroll_to_end() -> Task<Message> {
    scrollable::snap_to(ui::chat::scroll_id(), scrollable::RelativeOffset::END)
}

fn nav_button(label: &str, target: Screen, current: Screen) -> Element<'_, Message> {
    let active = target == current;
    button(label)
        .on_press(Message::Navigate(target))
        .style(move |theme: &Theme, status| {
            if active {
                button::primary(theme, status)
            } else {
                button::secondary(theme, status)
            }
        })
        .into()
}
