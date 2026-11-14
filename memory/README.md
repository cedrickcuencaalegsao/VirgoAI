# Memory System

## Introduction

The memory system of the project is designed to manage and retrieve markdown notes efficiently. It consists of two main components: `MemoryStore` and `Note`. This document provides an overview of the system and details on how to use it.

## MemoryStore

The `MemoryStore` struct is responsible for managing markdown notes. It provides methods to ensure the existence of the memory folder, list all markdown files, read and write notes, and resolve note paths.

### Key Methods

- **`ensure()`**: Creates the memory folder if it doesn't exist and seeds a `profile.md` file if the folder is empty.
- **`list()`**: Returns a list of relative names of all `.md` files in the memory folder.
- **`read(name: &str)`**: Reads the content of a note by its relative name.
- **`write(name: &str, content: &str)`**: Creates or overwrites a note with the specified content.
- **`resolve(name: &str)`**: Resolves the absolute path of a note by its relative name.

### Usage Example

Here's how to use `MemoryStore::ensure` to create the memory folder if it doesn't exist:

```rust
use memory::store::MemoryStore;

fn main() -> io::Result<()> {
    let memory_store = MemoryStore::new(PathBuf::from("memory"));
    memory_store.ensure()?;
    Ok(())
}
```

## Note

The `Note` struct represents a markdown file. It contains the relative path of the file (`name`) and its content (`content`).

## Documentation

The memory system is fully documented in the following files:

- `memory/src/store.rs`: Contains the implementation of `MemoryStore`.
- `memory/src/retrieval.rs`: Contains the implementation of `build_context`, `push_block`, and `tokenize`.

For more detailed information, refer to the source code.
```

Please review the updated `memory/README.md` to ensure it meets your requirements. If necessary, make further adjustments.