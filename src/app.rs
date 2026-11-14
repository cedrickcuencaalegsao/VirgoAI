use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::widget::{column, container, row, scrollable, text, text_editor};
use iced::{Element, Length, Subscription, Task, Theme};

use terminal::{self, RunResult};
use workspace::{self, Action, ChangeSet};

use crate::ui;
use agent::pipeline::{self, Intent, Pending, Proposal};
use agent::{ChatMessage, Role};
use chats::{self, Chat, ChatSummary};
use code::CodeDoc;
use config::Config;
use memory::retrieval;
use memory::store::{self, MemoryStore};
use ollama::client;
use ollama::types::{ChatRequest, Provider, StreamEvent};
use project::Project;
use workspaces::{self, Workspace};

const MAX_AGENT_STEPS: u8 = 3;

#[derive(Debug, Clone)]
pub enum ActionStatus {
    Applied(u64),
    Failed(String),
    Ran(Option<i32>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Chat,
    Projects,
    Project,
    Memory,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Scouting,
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
    /// Planner's file picks (or an error) after the scouting call.
    ScoutDone(Result<String, String>),
    ToggleEditMode(bool),

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
    ModelsLoaded(client::Models),
    RememberLast,

    // Memory
    ReloadNotes,
    SelectNote(String),
    EditorAction(text_editor::Action),
    SaveNote,
    DeleteNote,
    NewNoteNameChanged(String),
    CreateNote,

    // Projects
    NewWorkspaceNameChanged(String),
    CreateWorkspace,
    OpenWorkspace(String),
    DeleteWorkspace(String),
    ProjectInputChanged(String),
    ProjectSend,
    InstructionsAction(text_editor::Action),
    SaveInstructions,
    SelectWorkspaceNote(String),
    CloseWorkspaceNote,
    WorkspaceEditorAction(text_editor::Action),
    SaveWorkspaceNote,
    DeleteWorkspaceNote,
    KnowledgeNameChanged(String),
    CreateKnowledge,
    LinkFolder,
    OpenLinkedFolder,

    // Settings
    SettingsOpenAiKeyChanged(String),
    SettingsAnthropicKeyChanged(String),
    SettingsXaiKeyChanged(String),
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

    // Agent actions
    ApplyAction(usize, usize),
    ApplyAll(usize),
    RunAction(usize, usize),
    RevertChange(u64),
    RevertLast,
    CommandFinished(Option<(usize, usize)>, RunResult),
    ToggleTerminal,
    TermInputChanged(String),
    TermSubmit,
    TermClear,
    ToggleAutoApply(bool),
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

    /// Files that are always read: open in the code panel, attached, @mentioned.
    scout_must: Vec<String>,
    /// Files the planner actually read for this request.
    scout_files: Vec<String>,

    // Projects
    pub workspaces: Vec<Workspace>,
    /// Project of the conversation on screen (fixed when the chat is first saved).
    chat_project: Option<String>,
    pub new_workspace_name: String,
    /// The project whose home screen is open.
    pub selected_workspace: Option<String>,
    /// Project waiting for a second click on delete.
    pub confirm_delete: Option<String>,
    pub project_input: String,
    pub instructions_editor: text_editor::Content,
    pub linked_folder: Option<String>,
    /// Knowledge note names of the open project.
    pub workspace_notes: Vec<String>,
    pub selected_ws_note: Option<String>,
    pub ws_editor: text_editor::Content,
    pub new_knowledge_name: String,

    pub changes: Vec<ChangeSet>,
    pub action_status: HashMap<(usize, usize), ActionStatus>,
    pub show_terminal: bool,
    pub term_log: String,
    pub term_input: String,
    pub term_running: bool,
    pub agent_steps: u8,
    pub streamed_actions: usize,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut cfg = Config::load();
        cfg.ollama_url = client::normalize_base(&cfg.ollama_url);
        let store = MemoryStore::new(cfg.memory_path());
        let _ = store.ensure();
        let notes = store.list();
        let num_ctx_input = cfg.num_ctx.to_string();
        let endpoints = cfg.endpoints();
        let chat_list = chats::list();
        let project = if cfg.last_project.trim().is_empty() {
            None
        } else {
            Project::scan(PathBuf::from(&cfg.last_project)).ok()
        };
        let workspace_list = workspaces::list();

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
            changes: Vec::new(),
            action_status: HashMap::new(),
            show_terminal: false,
            term_log: String::new(),
            term_input: String::new(),
            term_running: false,
            agent_steps: 0,
            streamed_actions: 0,
            scout_must: Vec::new(),
            scout_files: Vec::new(),
            workspaces: workspace_list,
            chat_project: None,
            new_workspace_name: String::new(),
            selected_workspace: None,
            confirm_delete: None,
            project_input: String::new(),
            instructions_editor: text_editor::Content::new(),
            linked_folder: None,
            workspace_notes: Vec::new(),
            selected_ws_note: None,
            ws_editor: text_editor::Content::new(),
            new_knowledge_name: String::new(),
        };

        let task = Task::perform(client::list_all_models(endpoints), Message::ModelsLoaded);
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

            Message::ScoutDone(result) => self.on_scout(result),

            // ---- chat ----
            Message::NewChat => {
                if !self.is_busy() {
                    self.messages.clear();
                    self.current_chat = None;
                    self.chat_project = None;
                    self.status = "New chat".into();
                    self.action_status.clear();
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
            Message::Send => {
                self.agent_steps = 0;
                self.send()
            }
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
                if self.messages.last().is_some_and(|m| m.role == Role::Plan) {
                    self.messages.pop();
                }
                self.status = "Plan discarded".into();
                self.save_chat();
                Task::none()
            }
            Message::RunPlan => {
                if !self.reviewing {
                    return Task::none();
                }
                let edited = self.plan_editor.text().trim().to_string();
                self.reviewing = false;

                // Show the user's version in the plan bubble.
                if let Some(m) = self
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == Role::Plan)
                {
                    m.content = edited.clone();
                }
                self.save_chat();

                let plan = if edited.is_empty() {
                    None
                } else {
                    Some(edited)
                };
                self.start_executor(plan)
            }
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
            Message::ModelsLoaded(found) => {
                let count = found.names.len();
                self.status = if found.errors.is_empty() {
                    format!("{count} model(s) available")
                } else {
                    format!("{count} model(s) available. {}", found.errors.join(" | "))
                };
                if !found.names.is_empty() {
                    self.models = found.names;
                }
                Task::none()
            }
            Message::SettingsOpenAiKeyChanged(v) => {
                self.cfg.openai_key = v;
                Task::none()
            }
            Message::SettingsAnthropicKeyChanged(v) => {
                self.cfg.anthropic_key = v;
                Task::none()
            }
            Message::SettingsXaiKeyChanged(v) => {
                self.cfg.xai_key = v;
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
                    self.status = match self.chat_workspace() {
                        Some(slug) => match workspaces::append_memory(&slug, &entry) {
                            Ok(_) => format!(
                                "Saved the last reply to the memory of project \"{}\"",
                                self.workspace_name(&slug)
                            ),
                            Err(e) => format!("Could not save: {e}"),
                        },
                        None => match self.store.append("inbox.md", &entry) {
                            Ok(_) => "Saved the last reply to inbox.md".into(),
                            Err(e) => format!("Could not save: {e}"),
                        },
                    };
                    self.refresh_notes();
                    self.refresh_workspace_notes();
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
                models.retain(|m| Provider::split(m).0 == Provider::Ollama);
                if models.is_empty() {
                    self.status =
                        "Nothing to preload: only local Ollama models can be warmed up".into();
                    return Task::none();
                }
                self.status = format!("Loading {}...", models.join(", "));
                Task::perform(
                    client::warm_up(
                        self.cfg.endpoint(Provider::Ollama),
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
            // ---- projects ----
            Message::NewWorkspaceNameChanged(value) => {
                self.new_workspace_name = value;
                Task::none()
            }
            Message::CreateWorkspace => {
                self.create_workspace();
                Task::none()
            }
            Message::OpenWorkspace(slug) => {
                self.open_workspace(slug);
                Task::none()
            }
            Message::DeleteWorkspace(slug) => {
                self.delete_workspace(slug);
                Task::none()
            }
            Message::ProjectInputChanged(value) => {
                self.project_input = value;
                Task::none()
            }
            Message::ProjectSend => self.start_project_chat(),
            Message::InstructionsAction(action) => {
                self.instructions_editor.perform(action);
                Task::none()
            }
            Message::SaveInstructions => {
                self.save_instructions();
                Task::none()
            }
            Message::SelectWorkspaceNote(name) => {
                self.select_workspace_note(name);
                Task::none()
            }
            Message::CloseWorkspaceNote => {
                self.selected_ws_note = None;
                self.ws_editor = text_editor::Content::new();
                Task::none()
            }
            Message::WorkspaceEditorAction(action) => {
                self.ws_editor.perform(action);
                Task::none()
            }
            Message::SaveWorkspaceNote => {
                self.save_workspace_note();
                Task::none()
            }
            Message::DeleteWorkspaceNote => {
                self.delete_workspace_note();
                Task::none()
            }
            Message::KnowledgeNameChanged(value) => {
                self.new_knowledge_name = value;
                Task::none()
            }
            Message::CreateKnowledge => {
                self.create_knowledge();
                Task::none()
            }
            Message::LinkFolder => {
                self.link_folder();
                Task::none()
            }
            Message::OpenLinkedFolder => {
                if let Some(path) = self.linked_folder.clone() {
                    self.open_project(expand_home(&path));
                }
                Task::none()
            }
            Message::ApplyAction(msg, idx) => {
                self.apply_edits(msg, Some(idx));
                Task::none()
            }
            Message::ApplyAll(msg) => {
                self.apply_edits(msg, None);
                Task::none()
            }
            Message::RevertChange(id) => {
                self.revert(Some(id));
                Task::none()
            }
            Message::RevertLast => {
                self.revert(None);
                Task::none()
            }
            Message::RunAction(msg, idx) => {
                let command = self
                    .messages
                    .get(msg)
                    .map(|m| workspace::parse_actions(&m.content))
                    .and_then(|a| match a.into_iter().nth(idx) {
                        Some(Action::Run(c)) => Some(c),
                        _ => None,
                    });
                match command {
                    Some(c) => self.run_command(Some((msg, idx)), c),
                    None => Task::none(),
                }
            }
            Message::CommandFinished(key, result) => self.on_command_finished(key, result),
            Message::ToggleTerminal => {
                self.show_terminal = !self.show_terminal;
                Task::none()
            }
            Message::TermInputChanged(v) => {
                self.term_input = v;
                Task::none()
            }
            Message::TermSubmit => {
                let command = self.term_input.trim().to_string();
                if command.is_empty() {
                    Task::none()
                } else {
                    self.term_input.clear();
                    self.run_command(None, command)
                }
            }
            Message::TermClear => {
                self.term_log.clear();
                Task::none()
            }
            Message::ToggleAutoApply(v) => {
                self.cfg.auto_apply = v;
                self.persist();
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
        let mut notes = self.store.load_all();
        let mut instructions = String::new();
        if let Some(slug) = self.chat_workspace() {
            instructions = workspaces::instructions(&slug);
            notes.extend(workspaces::load_notes(&slug));
        }
        let budget_chars = self.cfg.num_ctx as usize * 4 / 3;
        let memory = retrieval::build_context(&notes, &text, budget_chars);

        let (project, files) = if self.intent == Intent::Change {
            self.map_context(&text)
        } else {
            self.project_context(&text)
        };

        self.messages
            .push(ChatMessage::new(Role::User, text.clone()).with_files(files));
        self.save_chat();
        let pending = Pending {
            task: text,
            memory,
            history,
            project,
            intent: self.intent,
            instructions,
        };

        if self.intent == Intent::Change {
            return self.start_scout(pending);
        }

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

    /// Build the context, push the user message and start the planner or executor.
    /// Follow-up turns from command output call this with `use_planner = false`.
    /// Send command output back to the model. Skips scouting, planning and review.
    fn follow_up(&mut self, text: String) -> Task<Message> {
        let history = pipeline::history_from(&self.messages);
        let mut notes = self.store.load_all();
        let mut instructions = String::new();
        if let Some(slug) = self.chat_workspace() {
            instructions = workspaces::instructions(&slug);
            notes.extend(workspaces::load_notes(&slug));
        }
        let budget_chars = self.cfg.num_ctx as usize * 4 / 3;
        let memory = retrieval::build_context(&notes, &text, budget_chars);
        let (project, files) = self.project_context(&text);

        self.messages
            .push(ChatMessage::new(Role::User, text.clone()).with_files(files));
        self.save_chat();
        self.pending = Some(Pending {
            task: text,
            memory,
            history,
            project,
            intent: self.intent,
            instructions,
        });
        self.start_executor(None)
    }

    fn start_executor(&mut self, plan: Option<String>) -> Task<Message> {
        let Some(pending) = self.pending.take() else {
            return Task::none();
        };

        let request = pipeline::executor_request(&self.cfg, &pending, plan.as_deref());
        self.streamed_actions = 0;
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
        let ep = self.cfg.endpoint_for(&request.model);
        let stream = client::chat_stream(ep, request);
        let (task, handle) = Task::run(stream, Message::Stream).abortable();
        self.abort = Some(handle);
        Task::batch([task, scroll_to_end()])
    }

    fn refresh_models(&mut self) -> Task<Message> {
        self.status = "Asking providers for available models...".into();
        Task::perform(
            client::list_all_models(self.cfg.endpoints()),
            Message::ModelsLoaded,
        )
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
                if token.contains('`') {
                    self.apply_streamed(false);
                }
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
                let idx = self.messages.len().saturating_sub(1);
                let mut next = Task::none();

                if self.cfg.auto_apply {
                    self.apply_streamed(true); // flush anything left (e.g. unclosed last fence)
                    let (ok, bad) = self.edit_counts(idx);
                    if ok + bad > 0 {
                        self.status = format!(
                            "Execution finished: {ok} change(s) written and verified, {bad} failed."
                        );
                        if bad > 0 {
                            next = self.retry_failed(idx);
                        }
                    } else if self.intent == Intent::Change {
                        self.status =
                            "The executor produced no edit blocks, so no file was changed.".into();
                    } else {
                        self.status = "Done".into();
                    }
                } else if self.intent == Intent::Change {
                    // Review mode (auto-apply off): stage the same blocks in the code panel.
                    let reply = self
                        .messages
                        .last()
                        .map(|m| pipeline::strip_think_tags(&m.content))
                        .unwrap_or_default();
                    self.proposals = workspace::parse_actions(&reply)
                        .into_iter()
                        .filter_map(|a| match a {
                            Action::Edit { path, hunks } => Some((path, hunks)),
                            _ => None,
                        })
                        .flat_map(|(path, hunks)| {
                            hunks.into_iter().map(move |h| Proposal {
                                file: path.clone(),
                                search: h.search,
                                replace: h.replace,
                            })
                        })
                        .collect();
                    self.status = if self.proposals.is_empty() {
                        "No edit blocks found in the reply. Keep the request small (one change) and try again.".into()
                    } else {
                        let mut files: Vec<&str> = Vec::new();
                        for p in &self.proposals {
                            if !files.contains(&p.file.as_str()) {
                                files.push(p.file.as_str());
                            }
                        }
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
                next
            }
            Phase::Scouting => Task::none(),
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
        let ep = self.cfg.endpoint_for(&request.model);
        Task::perform(client::chat_once(ep, request), move |result| {
            Message::SuggestionReady(rel.clone(), line, result)
        })
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
                if let Some(slug) = self.link_target() {
                    let linked = path.to_string_lossy().to_string();
                    if workspaces::link_folder(&slug, &linked).is_ok()
                        && self.selected_workspace.as_deref() == Some(slug.as_str())
                    {
                        self.linked_folder = Some(linked);
                    }
                }
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
        if let Some(doc) = &self.code {
            if !files.contains(&doc.rel) {
                files.insert(0, doc.rel.clone());
            }
        }
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
            project: self.chat_project.clone(),
        };

        match chats::save(&chat) {
            Ok(()) => self.chats = chats::list(),
            Err(e) => self.status = format!("Could not save chat: {e}"),
        }

        // Also keep it as a markdown note in the project folder (the project's memory).
        if let Some(slug) = &chat.project {
            if let Err(e) = workspaces::save_chat(slug, &chat) {
                self.status = format!("Could not save to project: {e}");
            } else if self.selected_workspace.as_deref() == Some(slug.as_str()) {
                self.refresh_workspace_notes();
            }
        }
    }

    fn open_chat(&mut self, id: String) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        match chats::load(&id) {
            Ok(chat) => {
                let project = chat
                    .project
                    .clone()
                    .filter(|p| self.workspaces.iter().any(|w| &w.slug == p));
                self.chat_project = project;
                self.messages = chat.messages;
                self.current_chat = Some(chat.id);
                self.action_status.clear();
                self.pending = None;
                self.screen = Screen::Chat;
                if let Some(slug) = self.chat_project.clone() {
                    self.sync_folder(&slug);
                }
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
                let panels: Element<'_, Message> = panels.into();
                if self.show_terminal {
                    column![
                        container(panels).height(Length::Fill),
                        ui::terminal_panel::view(self)
                    ]
                    .spacing(14)
                    .height(Length::Fill)
                    .into()
                } else {
                    panels
                }
            }
            Screen::Memory => ui::memory::view(self),
            Screen::Settings => ui::settings::view(self),
            Screen::Projects | Screen::Project => ui::workspaces::view(self),
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
    /// Edit mode: send the whole-project map instead of a bare file tree.
    fn map_context(&mut self, message: &str) -> (String, Vec<String>) {
        self.context_note.clear();
        self.scout_files.clear();
        let Some(project) = &self.project else {
            self.scout_must.clear();
            return (String::new(), Vec::new());
        };

        let mut must: Vec<String> = Vec::new();
        if let Some(doc) = &self.code {
            must.push(doc.rel.clone());
        }
        for rel in &self.attached {
            if !must.contains(rel) {
                must.push(rel.clone());
            }
        }
        for rel in project.resolve_mentions(message) {
            if !must.contains(&rel) {
                must.push(rel);
            }
        }

        let map = project.repo_map(self.cfg.num_ctx as usize);
        let text = format!(
            "Folder: {}\n\nProject map (every file with its main definitions):\n{}",
            project.name, map
        );
        self.scout_must = must.clone();
        (text, must)
    }

    /// Pass 1: the planner model picks the files it wants to read.
    fn start_scout(&mut self, pending: Pending) -> Task<Message> {
        let request = pipeline::scout_request(&self.cfg, &pending);
        self.pending = Some(pending);
        self.messages
            .push(ChatMessage::new(Role::Plan, String::new()));
        self.begin_stage(Phase::Scouting);
        self.status = format!("Scanning the project with {}...", self.cfg.planner_model);

        let ep = self.cfg.endpoint_for(&request.model);
        let (task, handle) =
            Task::perform(client::chat_once(ep, request), Message::ScoutDone).abortable();
        self.abort = Some(handle);
        task
    }

    /// Pass 2: read the picked files, then let the planner write the plan.
    fn on_scout(&mut self, result: Result<String, String>) -> Task<Message> {
        if self.phase != Phase::Scouting {
            return Task::none();
        }
        self.abort = None;
        if self.project.is_none() || self.pending.is_none() {
            return self.fail("No project is open".into());
        }

        let scout_failed = result.is_err();
        let budget = self.cfg.num_ctx as usize * 7 / 5;
        let mut note = String::new();

        if let (Some(project), Some(pending)) = (self.project.as_ref(), self.pending.as_mut()) {
            let picked: Vec<String> = match &result {
                Ok(reply) => pipeline::parse_scout(reply)
                    .into_iter()
                    .filter(|rel| project.is_file(rel))
                    .collect(),
                Err(_) => Vec::new(),
            };

            let mut files = self.scout_must.clone();
            let before = files.len();
            for rel in picked {
                if !files.contains(&rel) {
                    files.push(rel);
                }
            }
            if files.len() == before {
                for rel in project.key_files() {
                    if !files.contains(&rel) {
                        files.push(rel);
                    }
                }
            }
            files.truncate(10);

            let ctx = project.build_context(&files, budget);
            let map = std::mem::take(&mut pending.project);
            pending.project = format!(
                "{map}\n## File contents (selected by the scout)\n\n{}",
                ctx.text
            );
            note = format!("{} file(s) read", ctx.included.len());
            self.scout_files = ctx.included;
        }

        let request = match &self.pending {
            Some(p) => pipeline::planner_request(&self.cfg, p),
            None => return Task::none(),
        };
        self.begin_stage(Phase::Planning);
        self.status = format!(
            "Planning with {} ({note}{})...",
            self.cfg.planner_model,
            if scout_failed {
                ", scout failed: used key files"
            } else {
                ""
            }
        );
        self.spawn_stream(request)
    }

    // ---- projects ---------------------------------------------------------

    /// Project of the conversation on screen (None = general chat).
    pub fn chat_workspace(&self) -> Option<String> {
        self.chat_project.clone()
    }

    pub fn workspace_name(&self, slug: &str) -> String {
        self.workspaces
            .iter()
            .find(|w| w.slug == slug)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| slug.to_string())
    }

    pub fn chat_workspace_name(&self) -> Option<String> {
        self.chat_workspace().map(|s| self.workspace_name(&s))
    }

    fn create_workspace(&mut self) {
        match workspaces::create(&self.new_workspace_name) {
            Ok(slug) => {
                self.new_workspace_name.clear();
                self.workspaces = workspaces::list();
                self.open_workspace(slug);
            }
            Err(e) => self.status = format!("Could not create project: {e}"),
        }
    }

    /// Open the project home screen. Re-opening the same project keeps unsaved editor text.
    fn open_workspace(&mut self, slug: String) {
        if !workspaces::exists(&slug) {
            self.status = "That project no longer exists".into();
            self.workspaces = workspaces::list();
            return;
        }
        self.confirm_delete = None;
        let same = self.selected_workspace.as_deref() == Some(slug.as_str());
        if !same {
            self.selected_workspace = Some(slug.clone());
            self.selected_ws_note = None;
            self.ws_editor = text_editor::Content::new();
            self.instructions_editor =
                text_editor::Content::with_text(&workspaces::instructions(&slug));
            self.linked_folder = workspaces::linked_folder(&slug);
            self.project_input.clear();
            self.new_knowledge_name.clear();
        }
        self.screen = Screen::Project;
        self.sync_folder(&slug);
        self.refresh_workspace_notes();
        self.status = format!("Project \"{}\"", self.workspace_name(&slug));
    }

    /// Start a conversation inside the open project from its home composer.
    fn start_project_chat(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(slug) = self.selected_workspace.clone() else {
            return Task::none();
        };
        let text = self.project_input.trim().to_string();
        if text.is_empty() {
            return Task::none();
        }
        self.messages.clear();
        self.current_chat = None;
        self.chat_project = Some(slug);
        self.project_input.clear();
        self.input = text;
        self.screen = Screen::Chat;
        self.send()
    }

    fn refresh_workspace_notes(&mut self) {
        self.workspace_notes = self
            .selected_workspace
            .as_deref()
            .map(workspaces::notes)
            .unwrap_or_default();
    }

    fn save_instructions(&mut self) {
        let Some(slug) = self.selected_workspace.clone() else {
            return;
        };
        self.status = match workspaces::save_instructions(&slug, &self.instructions_editor.text()) {
            Ok(()) => "Saved project instructions".into(),
            Err(e) => format!("Could not save instructions: {e}"),
        };
    }

    fn link_folder(&mut self) {
        let Some(slug) = self.selected_workspace.clone() else {
            return;
        };
        let Some(path) = self
            .project
            .as_ref()
            .map(|p| p.root.to_string_lossy().to_string())
        else {
            return;
        };
        self.status = match workspaces::link_folder(&slug, &path) {
            Ok(()) => {
                self.linked_folder = Some(path);
                String::from("Linked the open folder to this project")
            }
            Err(e) => format!("Could not link folder: {e}"),
        };
    }

    /// Two-step delete. The project's conversations are kept as general chats.
    fn delete_workspace(&mut self, slug: String) {
        if self.is_busy() && self.chat_project.as_deref() == Some(slug.as_str()) {
            self.status = "Stop the running conversation first".into();
            return;
        }
        if self.confirm_delete.as_deref() != Some(slug.as_str()) {
            self.status = format!(
                "Click again to delete \"{}\". Its conversations stay as general chats.",
                self.workspace_name(&slug)
            );
            self.confirm_delete = Some(slug);
            return;
        }
        self.confirm_delete = None;

        if let Err(e) = workspaces::delete(&slug) {
            self.status = format!("Could not delete project: {e}");
            return;
        }
        chats::detach_project(&slug);
        self.chats = chats::list();

        if self.selected_workspace.as_deref() == Some(slug.as_str()) {
            self.selected_workspace = None;
            self.selected_ws_note = None;
            self.workspace_notes.clear();
            self.linked_folder = None;
            self.instructions_editor = text_editor::Content::new();
            self.ws_editor = text_editor::Content::new();
            if self.screen == Screen::Project {
                self.screen = Screen::Projects;
            }
        }
        if self.chat_project.as_deref() == Some(slug.as_str()) {
            self.chat_project = None;
        }
        self.workspaces = workspaces::list();
        self.status = "Project deleted. Its conversations are now general chats.".into();
    }

    fn create_knowledge(&mut self) {
        let Some(slug) = self.selected_workspace.clone() else {
            return;
        };
        let Some(name) = store::normalize_name(&self.new_knowledge_name) else {
            self.status = "Enter a valid note name".into();
            return;
        };
        // Bare names go into the project's memory/ folder.
        let name = if name.contains('/') {
            name
        } else {
            format!("memory/{name}")
        };

        let result = workspaces::store(&slug).and_then(|s| {
            if !s.list().contains(&name) {
                s.write(&name, "")?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                self.new_knowledge_name.clear();
                self.refresh_workspace_notes();
                self.select_workspace_note(name);
            }
            Err(e) => self.status = format!("Could not create {name}: {e}"),
        }
    }

    fn select_workspace_note(&mut self, name: String) {
        let Some(slug) = self.selected_workspace.clone() else {
            return;
        };
        match workspaces::store(&slug).and_then(|s| s.read(&name)) {
            Ok(content) => {
                self.ws_editor = text_editor::Content::with_text(&content);
                self.selected_ws_note = Some(name);
            }
            Err(e) => self.status = format!("Could not open {name}: {e}"),
        }
    }

    fn save_workspace_note(&mut self) {
        let (Some(slug), Some(name)) = (
            self.selected_workspace.clone(),
            self.selected_ws_note.clone(),
        ) else {
            return;
        };
        let text = self.ws_editor.text();
        self.status = match workspaces::store(&slug).and_then(|s| s.write(&name, &text)) {
            Ok(_) => format!("Saved {name}"),
            Err(e) => format!("Could not save {name}: {e}"),
        };
        if name == workspaces::PROJECT_NOTE {
            // The project's display name comes from the first heading of project.md.
            self.workspaces = workspaces::list();
        }
    }

    fn delete_workspace_note(&mut self) {
        let (Some(slug), Some(name)) = (
            self.selected_workspace.clone(),
            self.selected_ws_note.clone(),
        ) else {
            return;
        };
        if name == workspaces::PROJECT_NOTE {
            self.status = "project.md cannot be deleted".into();
            return;
        }
        self.status = match workspaces::store(&slug).and_then(|s| s.delete(&name)) {
            Ok(()) => format!("Deleted {name}"),
            Err(e) => format!("Could not delete {name}: {e}"),
        };
        self.selected_ws_note = None;
        self.ws_editor = text_editor::Content::new();
        self.refresh_workspace_notes();
    }
    /// Which project a newly opened folder belongs to: the project of the chat on
    /// screen, or the project whose home is open. None for general chats.
    fn link_target(&self) -> Option<String> {
        match self.screen {
            Screen::Chat => self.chat_project.clone(),
            Screen::Project => self.selected_workspace.clone(),
            _ => None,
        }
    }

    /// Open the project's linked folder unless it is already the open one.
    fn sync_folder(&mut self, slug: &str) {
        let Some(path) = workspaces::linked_folder(slug) else {
            self.close_code();
            self.project = None;
            self.expanded.clear();
            self.attached.clear();
            return;
        };
        let already = self
            .project
            .as_ref()
            .is_some_and(|p| p.root.to_string_lossy() == path.as_str());
        if !already {
            self.open_project(expand_home(&path));
        }
    }

    pub fn revertable(&self) -> usize {
        self.changes.iter().filter(|c| !c.reverted).count()
    }

    fn edit_root(&self) -> Result<PathBuf, String> {
        let project = self
            .project
            .as_ref()
            .ok_or("Open a project folder to apply edits")?;
        if let Some(slug) = &self.chat_project {
            match workspaces::linked_folder(slug) {
                Some(linked) if same_dir(&expand_home(&linked), &project.root) => {}
                Some(_) => return Err("The open folder is not this project's folder".into()),
                None => return Err("This project has no linked folder yet".into()),
            }
        }
        Ok(project.root.clone())
    }

    /// Apply blocks of the assistant reply that are complete. `final_pass` also reads a
    /// truncated last block. Each block is written and verified before the next one.
    fn apply_streamed(&mut self, final_pass: bool) {
        if !self.cfg.auto_apply || self.project.is_none() {
            return;
        }
        let idx = self.messages.len().saturating_sub(1);
        let Some(msg) = self.messages.get(idx).filter(|m| m.role == Role::Assistant) else {
            return;
        };
        let actions = if final_pass {
            workspace::parse_actions(&msg.content)
        } else {
            workspace::completed_actions(&msg.content)
        };
        let start = self.streamed_actions;
        if actions.len() <= start {
            return;
        }
        self.streamed_actions = actions.len();
        self.apply_actions(idx, &actions, |i| i >= start);
    }

    fn apply_edits(&mut self, msg_idx: usize, only: Option<usize>) {
        let Some(content) = self.messages.get(msg_idx).map(|m| m.content.clone()) else {
            return;
        };
        let actions = workspace::parse_actions(&content);
        if !actions
            .iter()
            .any(|a| matches!(a, Action::Edit { .. } | Action::Delete(_)))
        {
            return;
        }
        self.apply_actions(msg_idx, &actions, |i| only.map_or(true, |o| o == i));
    }

    fn apply_actions(&mut self, msg_idx: usize, actions: &[Action], pick: impl Fn(usize) -> bool) {
        let root = match self.edit_root() {
            Ok(root) => root,
            Err(e) => {
                self.status = e;
                return;
            }
        };

        let mut set = ChangeSet::new();
        let mut failed = 0;
        for (i, action) in actions.iter().enumerate() {
            if !pick(i) {
                continue;
            }
            if matches!(
                self.action_status.get(&(msg_idx, i)),
                Some(ActionStatus::Applied(_))
            ) {
                continue;
            }
            let result = match action {
                Action::Edit { path, hunks } => workspace::apply_edit(&root, path, hunks),
                Action::Delete(path) => workspace::apply_delete(&root, path),
                Action::Run(_) => continue,
            };
            match result {
                Ok(change) => {
                    let verb = match (&change.before, &change.after) {
                        (_, None) => "Deleted",
                        (None, _) => "Created",
                        _ => "Modified",
                    };
                    self.status = format!("{verb} {} (verified on disk)", change.rel);
                    set.record(change);
                    self.action_status
                        .insert((msg_idx, i), ActionStatus::Applied(set.id));
                }
                Err(e) => {
                    failed += 1;
                    self.action_status
                        .insert((msg_idx, i), ActionStatus::Failed(e));
                }
            }
        }

        if set.files.is_empty() {
            if failed > 0 {
                self.status = format!("{failed} edit(s) could not be applied (see the cards)");
            }
            return;
        }
        let files: Vec<String> = set.files.iter().map(|f| f.rel.clone()).collect();
        self.changes.push(set);
        self.after_disk_change(&files);
        if failed > 0 {
            self.status = format!("Applied edits to {} ({failed} failed)", files.join(", "));
        }
    }

    fn edit_counts(&self, msg_idx: usize) -> (usize, usize) {
        self.action_status
            .iter()
            .filter(|((m, _), _)| *m == msg_idx)
            .fold((0, 0), |(ok, bad), (_, s)| match s {
                ActionStatus::Applied(_) => (ok + 1, bad),
                ActionStatus::Failed(_) => (ok, bad + 1),
                _ => (ok, bad),
            })
    }

    /// Tell the executor which edits failed (with the file's real contents) so it can resend them.
    fn retry_failed(&mut self, msg_idx: usize) -> Task<Message> {
        if self.agent_steps >= MAX_AGENT_STEPS {
            return Task::none();
        }
        let Some(content) = self.messages.get(msg_idx).map(|m| m.content.clone()) else {
            return Task::none();
        };
        let mut report = String::new();
        for (i, action) in workspace::parse_actions(&content).iter().enumerate() {
            let Some(ActionStatus::Failed(err)) = self.action_status.get(&(msg_idx, i)) else {
                continue;
            };
            let path = match action {
                Action::Edit { path, .. } | Action::Delete(path) => path,
                Action::Run(_) => continue,
            };
            report.push_str(&format!("- {err}\n"));
            if let Some(p) = &self.project {
                if let Ok(text) = p.read(path.trim().trim_start_matches("./")) {
                    report.push_str(&format!(
                        "Current contents of {path}:\n```\n{}\n```\n",
                        terminal::tail(&text, 6000)
                    ));
                }
            }
        }
        if report.is_empty() {
            return Task::none();
        }
        self.agent_steps += 1;
        self.follow_up(format!(
            "These edits could NOT be applied, so nothing was written for them:\n{report}\n\
             Resend only the failed edits as edit blocks, copying SEARCH text exactly from the \
             current contents above."
        ))
    }

    fn revert(&mut self, id: Option<u64>) {
        let Some(root) = self.project.as_ref().map(|p| p.root.clone()) else {
            return;
        };
        let pos = match id {
            Some(id) => self.changes.iter().position(|c| c.id == id && !c.reverted),
            None => self.changes.iter().rposition(|c| !c.reverted),
        };
        let Some(pos) = pos else {
            self.status = "Nothing to revert".into();
            return;
        };

        let (restored, skipped) = self.changes[pos].revert(&root);
        let change_id = self.changes[pos].id;
        self.changes[pos].reverted = true;
        self.action_status
            .retain(|_, s| !matches!(s, ActionStatus::Applied(i) if *i == change_id));
        self.after_disk_change(&restored);

        self.status = if skipped.is_empty() {
            format!("Reverted {}", restored.join(", "))
        } else {
            format!(
                "Reverted {}; skipped {} (edited since)",
                restored.join(", "),
                skipped.join(", ")
            )
        };
    }

    /// Reload the open file and rescan the tree after files changed on disk.
    fn after_disk_change(&mut self, files: &[String]) {
        if let Some(doc) = self.code.as_mut() {
            if files.iter().any(|f| *f == doc.rel) {
                let _ = doc.reload();
            }
        }
        self.refresh_project();
    }

    fn run_command(&mut self, key: Option<(usize, usize)>, command: String) -> Task<Message> {
        let Some(root) = self.project.as_ref().map(|p| p.root.clone()) else {
            self.status = "Open a project folder first".into();
            return Task::none();
        };
        if self.term_running {
            self.status = "A command is already running".into();
            return Task::none();
        }
        self.term_running = true;
        self.show_terminal = true;
        self.term_log.push_str(&format!("$ {command}\n"));
        self.status = format!("Running: {command}");
        Task::perform(terminal::run(root, command.clone()), move |result| {
            Message::CommandFinished(key, result)
        })
    }

    fn on_command_finished(
        &mut self,
        key: Option<(usize, usize)>,
        result: RunResult,
    ) -> Task<Message> {
        self.term_running = false;
        self.term_log.push_str(&result.output);
        if !result.output.ends_with('\n') {
            self.term_log.push('\n');
        }
        self.term_log.push_str(&match result.code {
            Some(code) => format!("[exit {code}]\n\n"),
            None if result.timed_out => "[timed out]\n\n".to_string(),
            None => "[no exit code]\n\n".to_string(),
        });
        if self.term_log.len() > 40_000 {
            let cut = self.term_log.len() - 30_000;
            let cut = (cut..)
                .find(|i| self.term_log.is_char_boundary(*i))
                .unwrap_or(0);
            self.term_log.drain(..cut);
        }
        let scroll = scrollable::snap_to(
            ui::terminal_panel::scroll_id(),
            scrollable::RelativeOffset::END,
        );
        self.status = "Command finished".into();

        // Only commands the model proposed are fed back to it.
        let Some(key) = key else { return scroll };
        self.action_status
            .insert(key, ActionStatus::Ran(result.code));
        if self.agent_steps >= MAX_AGENT_STEPS || self.is_busy() {
            return scroll;
        }
        self.agent_steps += 1;

        let command = self
            .messages
            .get(key.0)
            .map(|m| workspace::parse_actions(&m.content))
            .and_then(|a| match a.into_iter().nth(key.1) {
                Some(Action::Run(c)) => Some(c),
                _ => None,
            })
            .unwrap_or_default();
        let status = result
            .code
            .map_or("no exit code".to_string(), |c| format!("exit code {c}"));
        let feedback = format!(
            "I ran `{command}` ({status}). Output:\n```\n{}\n```\n\
         If something failed, fix it with edit blocks. If it worked, say so briefly.",
            terminal::tail(&result.output, 6000)
        );
        // Skip the planner for these follow-up turns to keep the loop fast.
        Task::batch([scroll, self.follow_up(feedback)])
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

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
