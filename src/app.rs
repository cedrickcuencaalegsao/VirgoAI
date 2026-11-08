use std::time::{Duration, Instant};

use iced::widget::{column, container, row, scrollable, text, text_editor};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::agent::pipeline::{self, Pending};
use crate::agent::{ChatMessage, Role};
use crate::chats::{self, Chat, ChatSummary};
use crate::config::Config;
use crate::memory::retrieval;
use crate::memory::store::{self, MemoryStore};
use crate::ollama::client;
use crate::ollama::types::{ChatRequest, StreamEvent};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Chat,
    Memory,
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
    /// Animation clock, only runs while a model is working.
    Tick,

    // Chat
    NewChat,
    OpenChat(String),
    DeleteChat(String),
    InputChanged(String),
    Send,
    Stop,
    Stream(StreamEvent),
    ToggleThinking(usize),
    /// Put text (a whole reply or one code block) on the clipboard.
    Copy(String),
    PlannerSelected(String),
    ExecutorSelected(String),
    RefreshModels,
    ModelsLoaded(Result<Vec<String>, String>),
    RememberLast,

    // Memory
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
    ToggleExecutorThink(bool),
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
    /// Saved conversations, newest first.
    pub chats: Vec<ChatSummary>,
    /// Id of the conversation on screen (None until its first message is saved).
    pub current_chat: Option<String>,
    pub input: String,
    pub pending: Option<Pending>,
    pub abort: Option<iced::task::Handle>,

    /// Animation frame counter for the "Thinking..." dots.
    pub tick: u32,
    /// When the current stage started producing reasoning.
    pub think_started: Option<Instant>,
    /// True while the stream is inside an inline `<think>` block.
    inline_think: bool,

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
        let chat_list = chats::list();

        let app = App {
            cfg,
            num_ctx_input,
            store,
            screen: Screen::Chat,
            phase: Phase::Idle,
            status: "Ready".into(),
            models: Vec::new(),
            messages: Vec::new(),
            chats: chat_list,
            current_chat: None,
            input: String::new(),
            pending: None,
            abort: None,
            tick: 0,
            think_started: None,
            inline_think: false,
            notes,
            selected_note: None,
            editor: text_editor::Content::new(),
            new_note_name: String::new(),
        };

        let task = Task::perform(client::list_models(url), Message::ModelsLoaded);
        (app, task)
    }

    pub fn theme(&self) -> Theme {
        ui::style::theme()
    }

    /// The animation clock only runs while a model is working.
    pub fn subscription(&self) -> Subscription<Message> {
        if self.is_busy() {
            iced::time::every(Duration::from_millis(400)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
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
            Message::Tick => {
                self.tick = self.tick.wrapping_add(1);
                Task::none()
            }

            // ---- chat ----
            Message::NewChat => {
                if !self.is_busy() {
                    self.messages.clear();
                    self.current_chat = None;
                    self.status = "New chat".into();
                }
                self.screen = Screen::Chat;
                Task::none()
            }
            Message::OpenChat(id) => self.open_chat(id),
            Message::DeleteChat(id) => {
                self.delete_chat(id);
                Task::none()
            }
            Message::InputChanged(value) => {
                self.input = value;
                Task::none()
            }
            Message::Send => self.send(),
            Message::Stop => self.stop(),
            Message::Stream(event) => self.on_stream(event),
            Message::Copy(content) => {
                self.status = "Copied to clipboard".into();
                iced::clipboard::write(content)
            }
            Message::ToggleThinking(index) => {
                if let Some(m) = self.messages.get_mut(index) {
                    m.show_thinking = !m.show_thinking;
                }
                Task::none()
            }
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
            Message::ToggleExecutorThink(value) => {
                self.cfg.executor_think = value;
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
                self.status = format!("Loading {}...", models.join(", "));
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
    // chat pipeline: planner stream -> executor stream
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
        self.save_chat();
        let pending = Pending { task: text, memory, history };

        if self.cfg.use_planner {
            let request = pipeline::planner_request(&self.cfg, &pending);
            self.pending = Some(pending);
            self.messages.push(ChatMessage::new(Role::Plan, String::new()));
            self.begin_stage(Phase::Planning);
            self.status = format!("Planning with {}...", self.cfg.planner_model);
            self.spawn_stream(request)
        } else {
            self.pending = Some(pending);
            self.start_executor(None)
        }
    }

    fn start_executor(&mut self, plan: Option<String>) -> Task<Message> {
        let Some(pending) = self.pending.take() else {
            return Task::none();
        };

        let request = pipeline::executor_request(&self.cfg, &pending, plan.as_deref());
        self.messages.push(ChatMessage::new(Role::Assistant, String::new()));
        self.begin_stage(Phase::Executing);
        self.status = format!("Answering with {}...", self.cfg.executor_model);
        self.spawn_stream(request)
    }

    fn begin_stage(&mut self, phase: Phase) {
        self.phase = phase;
        self.think_started = None;
        self.inline_think = false;
    }

    fn spawn_stream(&mut self, request: ChatRequest) -> Task<Message> {
        let stream = client::chat_stream(self.cfg.ollama_url.clone(), request);
        let (task, handle) = Task::run(stream, Message::Stream).abortable();
        self.abort = Some(handle);
        Task::batch([task, scroll_to_end()])
    }

    fn on_stream(&mut self, event: StreamEvent) -> Task<Message> {
        // Late events after Stop or an error are ignored.
        if self.phase == Phase::Idle {
            return Task::none();
        }
        match event {
            StreamEvent::Thinking(chunk) => {
                self.append_thinking(&chunk);
                scroll_to_end()
            }
            StreamEvent::Token(token) => {
                self.append_content(&token);
                scroll_to_end()
            }
            StreamEvent::Done => self.on_stage_done(),
            StreamEvent::Error(error) => self.fail(error),
        }
    }

    fn on_stage_done(&mut self) -> Task<Message> {
        self.abort = None;
        self.finish_thinking();

        match self.phase {
            Phase::Planning => {
                let plan = self
                    .messages
                    .last()
                    .map(|m| pipeline::strip_think_tags(&m.content))
                    .unwrap_or_default();

                if plan.is_empty() {
                    // Keep the bubble only if it holds some reasoning.
                    let drop_it = self.messages.last().is_some_and(|m| m.thinking.is_empty());
                    if drop_it {
                        self.messages.pop();
                    }
                    self.start_executor(None)
                } else {
                    if let Some(m) = self.messages.last_mut() {
                        m.content = plan.clone();
                    }
                    self.start_executor(Some(plan))
                }
            }
            Phase::Executing => {
                self.phase = Phase::Idle;
                self.status = "Done".into();
                self.save_chat();
                Task::none()
            }
            Phase::Idle => Task::none(),
        }
    }

    fn stop(&mut self) -> Task<Message> {
        if let Some(handle) = self.abort.take() {
            handle.abort();
        }
        self.finish_thinking();
        self.phase = Phase::Idle;
        self.pending = None;
        self.status = "Stopped".into();
        self.save_chat();
        Task::none()
    }

    fn fail(&mut self, error: String) -> Task<Message> {
        // Drop an empty bubble left behind by a failed stream.
        let empty_bubble = matches!(
            self.messages.last(),
            Some(m) if matches!(m.role, Role::Assistant | Role::Plan)
                && m.content.is_empty()
                && m.thinking.is_empty()
        );
        if empty_bubble {
            self.messages.pop();
        }

        self.think_started = None;
        self.messages.push(ChatMessage::new(Role::Error, error.clone()));
        self.phase = Phase::Idle;
        self.pending = None;
        self.abort = None;
        self.status = error;
        self.save_chat();
        scroll_to_end()
    }

    // ---- chat history -----------------------------------------------------

    /// Write the current conversation to disk and refresh the sidebar list.
    fn save_chat(&mut self) {
        // Leave out empty bubbles left behind by a stopped or failed reply.
        let messages: Vec<ChatMessage> = self
            .messages
            .iter()
            .filter(|m| {
                !(matches!(m.role, Role::Assistant | Role::Plan)
                    && m.content.is_empty()
                    && m.thinking.is_empty())
            })
            .cloned()
            .collect();
        if messages.is_empty() {
            return;
        }

        let id = self
            .current_chat
            .get_or_insert_with(chats::new_id)
            .clone();
        let chat = Chat {
            id,
            title: chats::title_from(&messages),
            updated: chats::now_secs(),
            messages,
        };

        match chats::save(&chat) {
            Ok(()) => self.chats = chats::list(),
            Err(e) => self.status = format!("Could not save chat: {e}"),
        }
    }

    fn open_chat(&mut self, id: String) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        match chats::load(&id) {
            Ok(chat) => {
                self.messages = chat.messages;
                self.current_chat = Some(chat.id);
                self.pending = None;
                self.screen = Screen::Chat;
                self.status = format!("Opened \"{}\"", chat.title);
                scroll_to_end()
            }
            Err(e) => {
                self.status = format!("Could not open chat: {e}");
                Task::none()
            }
        }
    }

    fn delete_chat(&mut self, id: String) {
        if self.is_busy() {
            return;
        }
        match chats::delete(&id) {
            Ok(()) => {
                if self.current_chat.as_deref() == Some(id.as_str()) {
                    self.messages.clear();
                    self.current_chat = None;
                }
                self.status = "Chat deleted".into();
            }
            Err(e) => self.status = format!("Could not delete chat: {e}"),
        }
        self.chats = chats::list();
    }

    // ---- streaming text helpers ------------------------------------------

    fn current_mut(&mut self) -> Option<&mut ChatMessage> {
        self.messages
            .last_mut()
            .filter(|m| matches!(m.role, Role::Plan | Role::Assistant))
    }

    fn append_thinking(&mut self, chunk: &str) {
        if self.think_started.is_none() {
            self.think_started = Some(Instant::now());
        }
        if let Some(m) = self.current_mut() {
            if m.thinking.is_empty() {
                // Show live reasoning while the model is thinking.
                m.show_thinking = true;
            }
            m.thinking.push_str(chunk);
        }
    }

    /// Record how long the model thought for and collapse the reasoning panel.
    fn finish_thinking(&mut self) {
        if let Some(started) = self.think_started.take() {
            let secs = started.elapsed().as_secs_f32();
            if let Some(m) = self.current_mut() {
                m.think_secs = Some(secs);
                m.show_thinking = false;
            }
        }
    }

    fn push_visible(&mut self, piece: &str) {
        if piece.is_empty() {
            return;
        }
        // Skip the blank lines models emit right after their reasoning.
        let nothing_yet = self.current_mut().map_or(true, |m| m.content.is_empty());
        if piece.trim().is_empty() && nothing_yet {
            return;
        }
        self.finish_thinking();
        if let Some(m) = self.current_mut() {
            m.content.push_str(piece);
        }
    }

    /// Route a content token. Some models put reasoning inline between
    /// `<think>` and `</think>`; send that part to the reasoning panel.
    fn append_content(&mut self, token: &str) {
        const OPEN: &str = "<think>";
        const CLOSE: &str = "</think>";

        let mut rest = token.to_string();
        loop {
            if self.inline_think {
                match rest.find(CLOSE) {
                    Some(i) => {
                        let inner = rest[..i].to_string();
                        if !inner.is_empty() {
                            self.append_thinking(&inner);
                        }
                        rest = rest[i + CLOSE.len()..].to_string();
                        self.inline_think = false;
                        self.finish_thinking();
                    }
                    None => {
                        self.append_thinking(&rest);
                        return;
                    }
                }
            } else {
                match rest.find(OPEN) {
                    Some(i) => {
                        let before = rest[..i].to_string();
                        self.push_visible(&before);
                        rest = rest[i + OPEN.len()..].to_string();
                        self.inline_think = true;
                    }
                    None => {
                        self.push_visible(&rest);
                        return;
                    }
                }
            }
        }
    }

    // ---- misc helpers -----------------------------------------------------

    fn refresh_models(&mut self) -> Task<Message> {
        self.status = "Asking Ollama for installed models...".into();
        Task::perform(
            client::list_models(self.cfg.ollama_url.clone()),
            Message::ModelsLoaded,
        )
    }

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
        let page: Element<'_, Message> = match self.screen {
            Screen::Chat => ui::chat::view(self),
            Screen::Memory => ui::memory::view(self),
            Screen::Settings => ui::settings::view(self),
        };

        let footer = text(self.status.as_str()).size(12).style(ui::style::dim);

        let content = column![container(page).height(Length::Fill), footer]
            .spacing(8)
            .padding(18)
            .width(Length::Fill);

        row![ui::nav::view(self), content]
            .height(Length::Fill)
            .into()
    }
}

fn scroll_to_end() -> Task<Message> {
    scrollable::snap_to(ui::chat::scroll_id(), scrollable::RelativeOffset::END)
}