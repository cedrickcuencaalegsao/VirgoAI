use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::widget::{column, container, row, scrollable, text, text_editor};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::agent::pipeline::{self, Intent, Pending, Proposal};
use crate::agent::{ChatMessage, Role};
use crate::chats::{self, Chat, ChatSummary};
use crate::code::CodeDoc;
use crate::config::Config;
use crate::memory::retrieval;
use crate::memory::store::{self, MemoryStore};
use crate::ollama::client;
use crate::ollama::types::{ChatRequest, StreamEvent};
use crate::project::Project;
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

    ToggleEditMode(bool),

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

    PlanAction(text_editor::Action),
    RunPlan,
    DiscardPlan,
    /// Open this file and stage the proposed edits for it.
    ReviewChange(String),

    // Project panel
    TogglePanel,
    OpenFolder,
    FolderPicked(Option<PathBuf>),
    ProjectPathChanged(String),
    OpenProjectPath,
    RefreshProject,
    CloseProject,
    ToggleDir(String),
    ToggleAttach(String),
    ClearAttached,
    PickMention(String),

    // Code panel
    OpenFile(String),
    CloseCode,
    ReloadCode,
    CodeScrolled(scrollable::Viewport),
    SuggestLine(usize),
    SuggestInputChanged(String),
    SubmitSuggestion,
    CancelSuggest,
    /// (file, line, model reply or error)
    SuggestionReady(String, usize, Result<String, String>),
    RevertLine(usize),
    DiscardEdits,
    ApplyEdits,
    UndoApply,
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

    /// When on, every message uses the plan -> review -> code workflow.
    pub edit_mode: bool,

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

    /// What the last sent message asked for.
    pub intent: Intent,
    /// True while the user is reading or editing the plan.
    pub reviewing: bool,
    pub plan_editor: text_editor::Content,
    /// Edit blocks returned by the executor, waiting to be staged per file.
    pub proposals: Vec<Proposal>,

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

    // Project panel
    pub project: Option<Project>,
    /// Folders opened in the tree (relative paths).
    pub expanded: HashSet<String>,
    /// Files attached to every message until removed (relative paths).
    pub attached: BTreeSet<String>,
    pub project_path_input: String,
    pub show_panel: bool,
    /// Short note about included files, shown in the status line.
    context_note: String,

    // Code panel
    pub code: Option<CodeDoc>,
    pub code_scroll_y: f32,
    pub code_view_h: f32,
    /// Line whose suggestion bar is open.
    pub suggest_line: Option<usize>,
    pub suggest_input: String,
    /// Line with a model request in flight.
    pub suggesting: Option<usize>,
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
        let project = if cfg.last_project.trim().is_empty() {
            None
        } else {
            Project::scan(PathBuf::from(&cfg.last_project)).ok()
        };

        let app = App {
            cfg,
            num_ctx_input,
            store,
            edit_mode: false,
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
            intent: Intent::Answer,
            reviewing: false,
            plan_editor: text_editor::Content::new(),
            proposals: Vec::new(),
            tick: 0,
            think_started: None,
            inline_think: false,
            notes,
            selected_note: None,
            editor: text_editor::Content::new(),
            new_note_name: String::new(),
            project,
            expanded: HashSet::new(),
            attached: BTreeSet::new(),
            project_path_input: String::new(),
            show_panel: true,
            context_note: String::new(),
            code: None,
            code_scroll_y: 0.0,
            code_view_h: 900.0,
            suggest_line: None,
            suggest_input: String::new(),
            suggesting: None,
        };

        let task = Task::perform(client::list_models(url), Message::ModelsLoaded);
        (app, task)
    }

    pub fn theme(&self) -> Theme {
        ui::style::theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.is_working() {
            iced::time::every(Duration::from_millis(400)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    /// A model is streaming right now.
    pub fn is_working(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Streaming, or waiting for the user to review the plan.
    pub fn is_busy(&self) -> bool {
        self.phase != Phase::Idle || self.reviewing
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

            Message::ToggleEditMode(value) => {
                self.edit_mode = value;
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

            Message::PlanAction(action) => {
                self.plan_editor.perform(action);
                Task::none()
            }
            Message::DiscardPlan => {
                self.reviewing = false;
                self.pending = None;
                self.status = "Plan discarded".into();
                Task::none()
            }
            Message::RunPlan => self.run_plan(),
            Message::ReviewChange(file) => self.review_change(file),

            // ---- project panel ----
            Message::TogglePanel => {
                self.show_panel = !self.show_panel;
                Task::none()
            }
            Message::OpenFolder => Task::perform(pick_folder(), Message::FolderPicked),
            Message::FolderPicked(path) => {
                if let Some(path) = path {
                    self.open_project(path);
                }
                Task::none()
            }
            Message::ProjectPathChanged(value) => {
                self.project_path_input = value;
                Task::none()
            }
            Message::OpenProjectPath => {
                let raw = self.project_path_input.trim().to_string();
                if !raw.is_empty() {
                    self.open_project(expand_home(&raw));
                }
                Task::none()
            }
            Message::RefreshProject => {
                self.refresh_project();
                Task::none()
            }
            Message::CloseProject => {
                self.close_project();
                Task::none()
            }
            Message::ToggleDir(rel) => {
                if !self.expanded.remove(&rel) {
                    self.expanded.insert(rel);
                }
                Task::none()
            }
            Message::ToggleAttach(rel) => {
                if !self.attached.remove(&rel) {
                    self.attached.insert(rel);
                }
                Task::none()
            }
            Message::ClearAttached => {
                self.attached.clear();
                Task::none()
            }
            Message::PickMention(rel) => {
                self.pick_mention(rel);
                Task::none()
            }

            // ---- code panel ----
            Message::OpenFile(rel) => self.open_file(rel),
            Message::CloseCode => {
                self.close_code();
                Task::none()
            }
            Message::ReloadCode => {
                if let Some(doc) = self.code.as_mut() {
                    self.status = match doc.reload() {
                        Ok(()) => format!("Reloaded {}", doc.rel),
                        Err(e) => e,
                    };
                }
                Task::none()
            }
            Message::CodeScrolled(viewport) => {
                self.code_scroll_y = viewport.absolute_offset().y;
                self.code_view_h = viewport.bounds().height;
                Task::none()
            }
            Message::SuggestLine(line) => {
                if self.suggest_line != Some(line) {
                    self.suggest_input.clear();
                }
                self.suggest_line = Some(line);
                Task::none()
            }
            Message::SuggestInputChanged(value) => {
                self.suggest_input = value;
                Task::none()
            }
            Message::CancelSuggest => {
                self.suggest_line = None;
                self.suggest_input.clear();
                Task::none()
            }
            Message::SubmitSuggestion => self.submit_suggestion(),
            Message::SuggestionReady(rel, line, result) => {
                self.on_suggestion(rel, line, result);
                Task::none()
            }
            Message::RevertLine(line) => {
                if let Some(doc) = self.code.as_mut() {
                    doc.clear_edit(line);
                }
                Task::none()
            }
            Message::DiscardEdits => {
                if let Some(doc) = self.code.as_mut() {
                    doc.clear_all();
                    self.status = "Discarded all AI changes".into();
                }
                Task::none()
            }
            Message::ApplyEdits => {
                if let Some(doc) = self.code.as_mut() {
                    self.status = match doc.apply() {
                        Ok(0) => "Nothing to apply".into(),
                        Ok(count) => format!("Applied {count} change(s) to {}", doc.rel),
                        Err(e) => e,
                    };
                }
                Task::none()
            }
            Message::UndoApply => {
                if let Some(doc) = self.code.as_mut() {
                    self.status = match doc.undo_apply() {
                        Ok(()) => format!("Restored {} to before the last apply", doc.rel),
                        Err(e) => e,
                    };
                }
                Task::none()
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
        let raw = self.input.trim().to_string();
        if raw.is_empty() || self.is_busy() {
            return Task::none();
        }
        self.input.clear();

        // "/edit ..." forces the change workflow.
        let (forced, text) = match raw.strip_prefix("/edit ") {
            Some(rest) => (true, rest.trim().to_string()),
            None => (false, raw),
        };
        self.intent = if self.project.is_some() && (forced || self.edit_mode) {
            Intent::Change
        } else {
            pipeline::classify(&text, self.project.is_some())
        };
        self.proposals.clear();
        self.reviewing = false;

        // Gather context BEFORE adding the new message to the history.
        let history = pipeline::history_from(&self.messages);
        let notes = self.store.load_all();
        let budget_chars = self.cfg.num_ctx as usize * 4 / 3;
        let memory = retrieval::build_context(&notes, &text, budget_chars);

        let (project, files) = self.project_context(&text);

        self.messages
            .push(ChatMessage::new(Role::User, text.clone()).with_files(files));
        self.save_chat();
        let pending = Pending {
            task: text,
            memory,
            history,
            project,
            intent: self.intent,
        };

        // Code changes always get a plan, so there is something to review.
        if self.cfg.use_planner || self.intent == Intent::Change {
            let request = pipeline::planner_request(&self.cfg, &pending);
            self.pending = Some(pending);
            self.messages
                .push(ChatMessage::new(Role::Plan, String::new()));
            self.begin_stage(Phase::Planning);
            self.status = format!(
                "Planning with {}...{}",
                self.cfg.planner_model, self.context_note
            );
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
        self.messages
            .push(ChatMessage::new(Role::Assistant, String::new()));
        self.begin_stage(Phase::Executing);
        self.status = format!(
            "Answering with {}...{}",
            self.cfg.executor_model, self.context_note
        );
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

                if self.intent == Intent::Change {
                    if let Some(m) = self.messages.last_mut() {
                        m.content = plan.clone();
                    }
                    self.phase = Phase::Idle;
                    self.reviewing = true;
                    self.plan_editor = text_editor::Content::with_text(&plan);
                    self.status = if plan.is_empty() {
                        "The planner gave no plan. Write one below (or leave it empty), then press Apply plan.".into()
                    } else {
                        format!(
                            "Plan ready from {}. Edit it, then press Apply plan to run {}.",
                            self.cfg.planner_model, self.cfg.executor_model
                        )
                    };
                    self.save_chat();
                    return scroll_to_end();
                }

                if plan.is_empty() {
                    let drop_it = self.messages.last().is_some_and(|m| m.thinking.is_empty());
                    if drop_it {
                        self.messages.pop();
                    }
                    return self.start_executor(None);
                }

                if let Some(m) = self.messages.last_mut() {
                    m.content = plan.clone();
                }
                self.start_executor(Some(plan))
            }
            Phase::Executing => {
                self.phase = Phase::Idle;

                if self.intent == Intent::Change {
                    let reply = self
                        .messages
                        .last()
                        .map(|m| pipeline::strip_think_tags(&m.content))
                        .unwrap_or_default();
                    self.proposals = pipeline::parse_edits(&reply);

                    let mut files: Vec<&str> = Vec::new();
                    for p in &self.proposals {
                        if !files.contains(&p.file.as_str()) {
                            files.push(p.file.as_str());
                        }
                    }
                    self.status = if self.proposals.is_empty() {
                        "The model returned no edit blocks. Try rephrasing or edit the plan.".into()
                    } else {
                        format!(
                            "{} change(s) in {} file(s). Click a file below to review it.",
                            self.proposals.len(),
                            files.len()
                        )
                    };
                } else {
                    self.status = "Done".into();
                }

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
        self.reviewing = false;
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
        self.messages
            .push(ChatMessage::new(Role::Error, error.clone()));
        self.phase = Phase::Idle;
        self.pending = None;
        self.abort = None;
        self.status = error;
        self.save_chat();
        scroll_to_end()
    }

    /// The user pressed "Run plan": run the executor with the (edited) plan.
    fn run_plan(&mut self) -> Task<Message> {
        if !self.reviewing {
            return Task::none();
        }
        let plan = self.plan_editor.text().trim().to_string();
        self.reviewing = false;

        if let Some(m) = self.messages.last_mut() {
            if m.role == Role::Plan {
                m.content = plan.clone();
            }
        }
        self.add_plan_files(&plan);

        let plan = if plan.is_empty() { None } else { Some(plan) };
        self.start_executor(plan)
    }

    /// The executor can only match text it can see, so add the contents of
    /// every project file the plan mentions.
    fn add_plan_files(&mut self, plan: &str) {
        let budget = self.cfg.num_ctx as usize * 7 / 5;
        let (Some(project), Some(pending)) = (self.project.as_ref(), self.pending.as_mut()) else {
            return;
        };
        let files: Vec<String> = project
            .entries
            .iter()
            .filter(|e| !e.is_dir && plan.contains(&e.rel))
            .map(|e| e.rel.clone())
            .collect();
        if files.is_empty() {
            return;
        }
        let ctx = project.build_context(&files, budget);
        pending
            .project
            .push_str("\n## Files named in the plan (edit these)\n\n");
        pending.project.push_str(&ctx.text);
    }

    /// Open `file` in the code panel and stage its proposed edits as red/green rows.
    fn review_change(&mut self, file: String) -> Task<Message> {
        let task = self.open_file(file.clone());

        let mut staged = 0;
        let mut failed: Vec<String> = Vec::new();
        if let Some(doc) = self.code.as_mut() {
            if doc.rel == file {
                for p in self.proposals.iter().filter(|p| p.file == file) {
                    match doc.stage(&p.search, &p.replace) {
                        Ok(()) => staged += 1,
                        Err(e) => failed.push(e),
                    }
                }
            } else {
                return task; // open_file already set an error status
            }
        }

        self.status = if failed.is_empty() {
            format!("Staged {staged} change(s) in {file}. Press Apply to write them.")
        } else {
            format!(
                "Staged {staged} change(s) in {file}; {} block(s) failed ({}). Use the bulb to fix them by hand.",
                failed.len(),
                failed[0]
            )
        };
        task
    }

    // ---- code panel -------------------------------------------------------

    fn open_file(&mut self, rel: String) -> Task<Message> {
        let Some(project) = &self.project else {
            return Task::none();
        };
        match CodeDoc::open(&project.root, &rel) {
            Ok(doc) => {
                self.status = format!("Opened {rel}");
                self.code = Some(doc);
                self.code_scroll_y = 0.0;
                self.suggest_line = None;
                self.suggest_input.clear();
                self.suggesting = None;
                scrollable::snap_to(
                    ui::code_panel::scroll_id(),
                    scrollable::RelativeOffset::START,
                )
            }
            Err(e) => {
                self.status = e;
                Task::none()
            }
        }
    }

    fn close_code(&mut self) {
        self.code = None;
        self.suggest_line = None;
        self.suggest_input.clear();
        self.suggesting = None;
    }

    /// Ask the executor model to rewrite the selected line.
    fn submit_suggestion(&mut self) -> Task<Message> {
        if self.suggesting.is_some() {
            return Task::none();
        }
        let Some(line) = self.suggest_line else {
            return Task::none();
        };
        let Some(doc) = &self.code else {
            return Task::none();
        };
        if line >= doc.lines.len() {
            return Task::none();
        }

        let request = pipeline::line_edit_request(&self.cfg, doc, line, &self.suggest_input);
        let rel = doc.rel.clone();

        self.suggesting = Some(line);
        self.status = format!(
            "Asking {} about line {}...",
            self.cfg.executor_model,
            line + 1
        );
        Task::perform(
            client::chat_once(self.cfg.ollama_url.clone(), request),
            move |result| Message::SuggestionReady(rel.clone(), line, result),
        )
    }

    fn on_suggestion(&mut self, rel: String, line: usize, result: Result<String, String>) {
        self.suggesting = None;

        // The user may have opened another file while the model was working.
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        if doc.rel != rel {
            return;
        }

        match result {
            Ok(reply) => {
                let original = doc.lines.get(line).cloned().unwrap_or_default();
                match pipeline::parse_line_edit(&reply, &original) {
                    Ok(replacement) => {
                        if replacement.len() == 1 && replacement[0] == original {
                            self.status = format!("No change suggested for line {}", line + 1);
                        } else {
                            doc.set_edit(line, replacement);
                            self.suggest_line = None;
                            self.suggest_input.clear();
                            self.status = format!(
                                "Suggestion ready for line {} (shown in green). Apply it or revert it.",
                                line + 1
                            );
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
            Err(e) => self.status = format!("Suggestion failed: {e}"),
        }
    }

    // ---- project folder ---------------------------------------------------

    fn open_project(&mut self, path: PathBuf) {
        match Project::scan(path.clone()) {
            Ok(project) => {
                self.status = format!(
                    "Opened {} ({} items{})",
                    project.name,
                    project.entries.len(),
                    if project.truncated { ", truncated" } else { "" }
                );
                self.close_code();
                self.project = Some(project);
                self.expanded.clear();
                self.attached.clear();
                self.project_path_input.clear();
                self.show_panel = true;
                self.cfg.last_project = path.to_string_lossy().to_string();
                self.persist();
            }
            Err(e) => self.status = format!("Could not open folder: {e}"),
        }
    }

    /// Rescan the folder, keeping open folders and attachments that still exist.
    fn refresh_project(&mut self) {
        let Some(root) = self.project.as_ref().map(|p| p.root.clone()) else {
            return;
        };
        match Project::scan(root) {
            Ok(project) => {
                self.attached.retain(|rel| project.contains(rel));
                self.expanded.retain(|rel| project.contains(rel));
                self.status = format!("Refreshed {}", project.name);
                self.project = Some(project);
            }
            Err(e) => self.status = format!("Could not refresh: {e}"),
        }
    }

    fn close_project(&mut self) {
        self.close_code();
        self.project = None;
        self.expanded.clear();
        self.attached.clear();
        self.cfg.last_project.clear();
        self.persist();
        self.status = "Project closed".into();
    }

    /// Replace the `@token` being typed with the chosen path.
    fn pick_mention(&mut self, rel: String) {
        let start = self
            .input
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        self.input.truncate(start);
        self.input.push_str(&format!("@{rel} "));
    }

    /// Build the project part of the prompt: the folder tree plus the contents
    /// of attached and @mentioned files, within a size budget. Returns the text
    /// and the list of files that made it in.
    fn project_context(&mut self, message: &str) -> (String, Vec<String>) {
        self.context_note.clear();
        let Some(project) = &self.project else {
            return (String::new(), Vec::new());
        };

        let mut files: Vec<String> = self.attached.iter().cloned().collect();
        for rel in project.resolve_mentions(message) {
            if !files.contains(&rel) {
                files.push(rel);
            }
        }

        // About 35% of the context window, at roughly 4 characters per token.
        let budget = self.cfg.num_ctx as usize * 7 / 5;
        let ctx = project.build_context(&files, budget);
        let overview = project.tree_overview((budget / 4).min(3000));

        let mut text = format!("Folder: {}\n\nFile tree:\n{}", project.name, overview);
        if !ctx.text.is_empty() {
            text.push_str("\n## File contents\n\n");
            text.push_str(&ctx.text);
        }

        let mut parts = Vec::new();
        if !ctx.included.is_empty() {
            parts.push(format!("{} file(s) included", ctx.included.len()));
        }
        if !ctx.skipped.is_empty() {
            parts.push(format!(
                "{} skipped: unreadable or over budget",
                ctx.skipped.len()
            ));
        }
        self.context_note = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        };

        (text, ctx.included)
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

        let id = self.current_chat.get_or_insert_with(chats::new_id).clone();
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
            Screen::Chat => {
                let chat_width = if self.code.is_some() {
                    Length::FillPortion(2)
                } else {
                    Length::Fill
                };
                let mut panels = row![container(ui::chat::view(self))
                    .width(chat_width)
                    .height(Length::Fill)]
                .spacing(14)
                .height(Length::Fill);

                if self.code.is_some() {
                    panels = panels.push(
                        container(ui::code_panel::view(self))
                            .width(Length::FillPortion(3))
                            .height(Length::Fill),
                    );
                }
                if self.show_panel {
                    panels = panels.push(ui::project_panel::view(self));
                }
                panels.into()
            }
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

/// Native "choose folder" dialog.
async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Open project folder")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
}

/// Turn a typed path into a real one: strips quotes and expands a leading `~/`.
fn expand_home(raw: &str) -> PathBuf {
    let raw = raw.trim().trim_matches(|c| c == '"' || c == '\'');
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}
