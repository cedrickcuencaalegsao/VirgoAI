# Agent Project Documentation

## Table of Contents

1. **lib.rs**
2. **pipeline.rs**
3. **Cargo.toml**

---

## lib.rs

### Module and Structs

- **Module: pipeline**
  - **Struct: ChatMessage**
    - **Fields:**
      - `role`: Role (User/Plan/Assistant/Error)
      - `content`: String
      - `thinking`: String (default empty)
      - `think_secs`: Option<f32> (default None)
      - `show_thinking`: bool (default false)
      - `files`: Vec<String> (default empty)
    - **Methods:**
      - `new(role: Role, content: impl Into<String>) -> Self`
      - `with_files(mut self, files: Vec<String>) -> Self`

- **Struct: Role**
  - **Variants:**
    - User
    - Plan
    - Assistant
    - Error

### Functions

- **Function: classify(text: &str, has_project: bool) -> Intent**
  - **Logic:**
    - Determines the request type (Answer/Change) based on the text and whether a project is present.

- **Function: parse_edits(reply: &str, known: &[String], default_file: Option<&str>) -> Vec<Proposal>**
  - **Logic:**
    - Parses `FILE:` + SEARCH/REPLACE blocks from the executor's reply.

- **Function: file_from_line(t: &str, known: &[String]) -> Option<String>**
  - **Logic:**
    - Identifies the file from a line that names a file.

### Structs and Enums

- **Struct: Proposal**
  - **Fields:**
    - `file`: String
    - `search`: String
    - `replace`: String

- **Enum: Intent**
  - **Variants:**
    - Answer
    - Change

---

## pipeline.rs

### Module and Structs

- **Struct: Pending**
  - **Fields:**
    - `task`: String
    - `memory`: String
    - `history`: Vec<ChatMsg>
    - `project`: String
    - `intent`: Intent
    - `instructions`: String

- **Struct: ChatMsg**
  - **Methods:**
    - `user(content: impl Into<String>) -> Self`
    - `assistant(content: impl Into<String>) -> Self`
    - `system(content: impl Into<String>) -> Self`

- **Struct: ChatRequest**
  - **Fields:**
    - `messages`: Vec<ChatMsg>
    - `system`: String
    - `options`: Options

- **Struct: Options**
  - **Fields:**
    - `temperature`: f32
    - `max_tokens`: usize

### Enums

- **Enum: StreamEvent**
  - **Variants:**
    - Message: ChatMsg
    - Error: String

### Functions

- **Function: planner_request(cfg: &Config, pending: &Pending) -> ChatRequest**
  - **Logic:**
    - Constructs a chat request for the planner stage.

- **Function: executor_request(cfg: &Config, pending: &Pending, plan: Option<&str>) -> ChatRequest**
  - **Logic:**
    - Constructs a chat request for the executor stage.

- **Function: line_edit_request(cfg: &Config, pending: &Pending) -> ChatRequest**
  - **Logic:**
    - Constructs a chat request for line editing.

- **Function: parse_line_edit(reply: &str, original: &str) -> Result<Vec<String>, String>**
  - **Logic:**
    - Parses the reply from the line editing stage.

- **Function: fix_indent(original: &str, replacement: Vec<String>) -> Vec<String>**
  - **Logic:**
    - Fixes the indentation of the replacement lines.

- **Function: append_memory(system: &mut String, memory: &str)**
  - **Logic:**
    - Appends memory to the system prompt.

- **Function: append_instructions(system: &mut String, instructions: &str)**
  - **Logic:**
    - Appends instructions to the system prompt.

- **Function: append_project(system: &mut String, project: &str)**
  - **Logic:**
    - Appends project details to the system prompt.

- **Function: strip_think_tags(text: &str) -> String**
  - **Logic:**
    - Strips think tags from the text.

---

## Cargo.toml

### Package Metadata

- **Name:** agent
- **Version:** 0.1.0
- **Edition:** 2024

### Dependencies

- **serde:** { version = "1", features = ["derive"] }
- **code:** { path = "../code" }
- **config:** { path = "../config" }
- **ollama:** { path = "../ollama" }

### Module Structure

- **Modules:**
  - pipeline
  - code
  - config
  - ollama
  - README.md