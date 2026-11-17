//! Step-by-step execution of a structured plan (see `agent::plan`).
//!
//! This is a child module of the crate root, so it can use the private fields and
//! methods of `App` (`begin_stage`, `spawn_stream`, `apply_edits`, ...).
//!
//! Flow:
//!   planner reply -> `finish_planning` -> parse -> validate against the project
//!     -> (one repair call if the checker found problems) -> user review
//!     -> `run_structured_plan` -> `start_next_step` -> executor stream
//!     -> `on_step_done` -> apply + verify on disk -> next step / retry / stop

use agent::pipeline::{self, StepInput};
use agent::plan::{self, MAX_STEP_TRIES, Plan, PlanRun, StepStatus};
use agent::{ChatMessage, Role};
use iced::Task;
use iced::widget::text_editor;
use ollama::client;

use super::{ActionStatus, App, Message, Phase, scroll_to_end};

impl App {
    // -----------------------------------------------------------------------
    // Planning: parse, validate, repair, review
    // -----------------------------------------------------------------------

    /// The planner finished a change plan. `raw` is its reply without `<think>` blocks.
    /// `repaired` is true once the repair call has already been made (only one is allowed).
    pub(crate) fn finish_planning(&mut self, raw: String, repaired: bool) -> Task<Message> {
        match plan::parse(&raw) {
            Ok(parsed) => {
                let problems = self.check_plan(&parsed);
                if !problems.is_empty() && !repaired {
                    return self.start_repair(raw, problems);
                }
                self.review_structured_plan(parsed, problems)
            }
            Err(e) => {
                if !repaired {
                    return self.start_repair(raw, vec![e]);
                }
                // Not JSON even after the repair: fall back to a free-text plan.
                self.review_plain_plan(raw)
            }
        }
    }

    /// Check a plan against the open project (paths exist, anchors are real lines).
    fn check_plan(&self, parsed: &Plan) -> Vec<String> {
        let Some(project) = self.project.as_ref() else {
            return Vec::new();
        };
        plan::validate(parsed, &|rel| project.read(rel).ok(), &|rel| {
            project.is_file(rel)
        })
    }

    /// Ask the planner once to fix the problems the checker found.
    fn start_repair(&mut self, raw: String, problems: Vec<String>) -> Task<Message> {
        let Some(pending) = self.pending.as_ref() else {
            return self.review_plain_plan(raw);
        };
        let request =
            pipeline::repair_request(&self.cfg, &pending.task, &pending.project, &raw, &problems);

        // The phase stays `Planning`, so Stop and the status strip keep working.
        self.status = format!(
            "Checking the plan: {} problem(s). Asking {} to fix them...",
            problems.len(),
            self.cfg.planner_model
        );
        let ep = self.cfg.endpoint_for(&request.model);
        let (task, handle) =
            Task::perform(client::chat_once(ep, request), Message::PlanRepaired).abortable();
        self.abort = Some(handle);
        task
    }

    /// Result of the repair call.
    pub(crate) fn on_plan_repaired(&mut self, result: Result<String, String>) -> Task<Message> {
        if self.phase != Phase::Planning {
            return Task::none(); // stopped in the meantime
        }
        self.abort = None;

        // The bubble still holds the planner's original reply.
        let original = self
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let mut failure = None;
        let text = match result {
            Ok(reply) => {
                let fixed = pipeline::strip_think_tags(&reply);
                if plan::parse(&fixed).is_ok() {
                    fixed
                } else {
                    failure = Some("the repaired plan was not valid JSON".to_string());
                    original
                }
            }
            Err(e) => {
                failure = Some(e);
                original
            }
        };

        let task = self.finish_planning(text, true);
        if let Some(e) = failure {
            self.status.push_str(&format!(" (plan repair failed: {e})"));
        }
        task
    }

    /// Show a parsed plan for review. The editor holds the plan as JSON, so what the
    /// user edits is exactly what runs.
    fn review_structured_plan(&mut self, parsed: Plan, warnings: Vec<String>) -> Task<Message> {
        let (task, instructions) = self
            .pending
            .as_ref()
            .map(|p| (p.task.clone(), p.instructions.clone()))
            .unwrap_or_default();

        if let Some(m) = self.messages.last_mut() {
            m.content = plan::render_markdown(&parsed);
        }
        self.plan_editor = text_editor::Content::with_text(&plan::to_pretty_json(&parsed));

        let steps = parsed.steps.len();
        self.status = match warnings.first() {
            None => format!(
                "Plan ready: {steps} step(s), checked against the project. Edit it if you like, \
                 then press Run plan to execute it step by step."
            ),
            Some(first) => format!(
                "Plan ready with {} unresolved warning(s): {first} Edit it or run it anyway.",
                warnings.len()
            ),
        };
        self.plan_run = Some(PlanRun::new(parsed, task, instructions, warnings));
        self.phase = Phase::Idle;
        self.reviewing = true;
        self.save_chat();
        scroll_to_end()
    }

    /// The old behaviour: a free-text plan that goes to the executor as a whole.
    pub(crate) fn review_plain_plan(&mut self, plan_text: String) -> Task<Message> {
        self.plan_run = None;
        if let Some(m) = self.messages.last_mut() {
            m.content = plan_text.clone();
        }
        self.phase = Phase::Idle;
        self.reviewing = true;
        self.plan_editor = text_editor::Content::with_text(&plan_text);
        self.status = if plan_text.is_empty() {
            "The planner gave no plan. Write one below (or leave it empty), then press Run plan."
                .into()
        } else {
            format!(
                "Plan ready from {} (free text, not structured). Edit it, then press Run plan \
                 to run {}.",
                self.cfg.planner_model, self.cfg.executor_model
            )
        };
        self.save_chat();
        scroll_to_end()
    }

    // -----------------------------------------------------------------------
    // Executing: one step at a time
    // -----------------------------------------------------------------------

    /// "Run plan" with a structured plan: read the (possibly edited) JSON and start step 1.
    pub(crate) fn run_structured_plan(&mut self) -> Task<Message> {
        let edited = self.plan_editor.text();
        let parsed = match plan::parse(&edited) {
            Ok(p) => p,
            Err(e) => {
                self.status =
                    format!("The plan is not valid: {e}. Fix the JSON, then press Run plan.");
                return Task::none();
            }
        };
        let warnings = self.check_plan(&parsed);
        let Some(old) = self.plan_run.take() else {
            return Task::none();
        };
        let run = PlanRun::new(parsed, old.task, old.instructions, warnings);

        self.reviewing = false;
        self.pending = None;
        self.agent_steps = 0;
        if let Some(m) = self
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.role == Role::Plan)
        {
            m.content = plan::render_markdown(&run.plan);
        }
        self.plan_run = Some(run);
        self.save_chat();
        self.start_next_step()
    }

    /// Start the executor for the next waiting step, or finish the run.
    pub(crate) fn start_next_step(&mut self) -> Task<Message> {
        let Some(run) = self.plan_run.as_mut() else {
            return Task::none();
        };
        let Some(idx) = run.next_todo() else {
            return self.finish_plan_run();
        };
        run.begin(idx);
        let step = run.plan.steps[idx].clone();
        let total = run.plan.steps.len();
        let goal = run.plan.goal.clone();
        let task = run.task.clone();
        let instructions = run.instructions.clone();
        let done = run.done_lines();
        let tries = run.tries;
        let retry_error = run.last_error.take();

        // Read the step's file NOW: earlier steps may have changed it.
        let Some(project) = self.project.as_ref() else {
            return self.fail("No project is open".into());
        };
        let budget = self.cfg.num_ctx as usize * 2;
        let too_big = project
            .read(&step.file)
            .map_or(false, |t| t.chars().count() > budget);
        let file_ctx = if too_big {
            let mut needles: Vec<&str> = vec![step.anchor.as_str()];
            needles.extend(step.symbols.iter().map(String::as_str));
            needles.push(step.target.as_str());
            project
                .window(&step.file, &needles, 120)
                .unwrap_or_else(|| {
                    project
                        .build_context(std::slice::from_ref(&step.file), budget)
                        .text
                })
        } else {
            project
                .build_context(std::slice::from_ref(&step.file), budget)
                .text
        };
        let project_name = project.name.clone();

        let request = pipeline::step_request(
            &self.cfg,
            &StepInput {
                project_name: &project_name,
                instructions: &instructions,
                task: &task,
                goal: &goal,
                step: &step,
                total,
                done: &done,
                file_ctx: &file_ctx,
                retry_error: retry_error.as_deref(),
            },
        );

        self.streamed_actions = 0;
        self.messages
            .push(ChatMessage::new(Role::Assistant, String::new()));
        self.begin_stage(Phase::Executing);
        self.status = format!(
            "Step {}/{}: {} with {}{}",
            step.id,
            total,
            step.file,
            self.cfg.executor_model,
            if tries > 0 {
                format!(" (retry {tries})")
            } else {
                String::new()
            }
        );
        self.spawn_stream(request)
    }

    /// The executor stream of a step ended: apply, verify, then move on or retry.
    pub(crate) fn on_step_done(&mut self) -> Task<Message> {
        self.phase = Phase::Idle;
        let msg_idx = self.messages.len().saturating_sub(1);

        // A plan run is an approved plan, so edits are applied even when auto-apply is off.
        // Blocks already applied while streaming are skipped by `apply_edits`.
        if let Err(e) = self.edit_root() {
            return self.step_failed(e, false); // retrying cannot fix this
        }
        self.apply_edits(msg_idx, None);

        let (ok, bad) = self.edit_counts(msg_idx);
        let error = if bad > 0 {
            Some(self.step_errors(msg_idx))
        } else if ok == 0 {
            Some("The reply contained no edit block for this step.".to_string())
        } else {
            None
        };

        match error {
            None => {
                if let Some(run) = self.plan_run.as_mut() {
                    run.mark_done();
                }
                self.save_chat();
                self.start_next_step()
            }
            Some(e) => self.step_failed(e, true),
        }
    }

    /// The failed blocks of one reply, one per line.
    fn step_errors(&self, msg_idx: usize) -> String {
        let mut errors: Vec<(usize, &str)> = self
            .action_status
            .iter()
            .filter(|((m, _), _)| *m == msg_idx)
            .filter_map(|((_, i), s)| match s {
                ActionStatus::Failed(e) => Some((*i, e.as_str())),
                _ => None,
            })
            .collect();
        errors.sort();
        errors
            .iter()
            .map(|(_, e)| format!("- {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Retry the step (up to `MAX_STEP_TRIES`) or stop and wait for Retry / Skip.
    fn step_failed(&mut self, error: String, allow_retry: bool) -> Task<Message> {
        let Some(run) = self.plan_run.as_mut() else {
            return Task::none();
        };
        if allow_retry && run.tries < MAX_STEP_TRIES {
            run.retry(error);
            self.status = "The step failed. Retrying it with the file's current contents...".into();
            return self.start_next_step();
        }

        let id = run.cur.map_or(0, |i| run.plan.steps[i].id);
        let first = error.lines().next().unwrap_or("").to_string();
        run.mark_failed(error);
        self.status = format!("Step {id} failed: {first} Use Retry or Skip in the plan panel.");
        self.save_chat();
        scroll_to_end()
    }

    /// Plan panel: run the failed step again.
    pub(crate) fn retry_step(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(run) = self.plan_run.as_mut() else {
            return Task::none();
        };
        if !run.reopen_failed() {
            return Task::none();
        }
        self.start_next_step()
    }

    /// Plan panel: leave the failed step out and carry on.
    pub(crate) fn skip_step(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(run) = self.plan_run.as_mut() else {
            return Task::none();
        };
        if !run.skip_failed() {
            return Task::none();
        }
        self.start_next_step()
    }

    /// Stop or a stream error while a step runs: keep that step as failed so it can be retried.
    pub(crate) fn interrupt_plan_run(&mut self, why: &str) {
        if let Some(run) = self.plan_run.as_mut() {
            run.interrupt(why);
        }
    }

    /// No step is waiting any more. Summarise and offer the verify command.
    fn finish_plan_run(&mut self) -> Task<Message> {
        let Some(run) = self.plan_run.as_ref() else {
            return Task::none();
        };
        let total = run.status.len();
        let done = run.count(|s| *s == StepStatus::Done);
        let skipped = run.count(|s| *s == StepStatus::Skipped);
        let blocked = run.has_todo();
        let verify = run.verify_command();

        let mut status = if blocked {
            format!(
                "Plan stopped: {done}/{total} step(s) applied. The rest depend on a step that \
                 failed or was skipped."
            )
        } else if skipped > 0 {
            format!("Plan finished: {done}/{total} step(s) applied, {skipped} skipped.")
        } else {
            format!("Plan finished: all {total} step(s) applied and verified on disk.")
        };

        // Commands only run when the user presses Run, so just prepare it.
        if !blocked {
            if let Some(command) = verify {
                self.term_input = command.clone();
                self.show_terminal = true;
                status.push_str(&format!(" Press Run in the terminal to check: {command}"));
            }
        }
        self.status = status;
        self.save_chat();
        scroll_to_end()
    }
}
