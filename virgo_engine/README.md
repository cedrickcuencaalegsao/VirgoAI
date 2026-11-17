# VirgoEngine

**Native Rust LLM Inference Engine with Intelligent Layer Streaming**

Version: 0.1.0  
Status: Architecture Foundation / Initial Development  
Language: Rust  
Primary Target: macOS (Apple Silicon)  
Initial Model Target: Qwen 2 / Qwen 2.5

---

# 1. Project Overview

VirgoEngine is an independent, native Rust inference engine designed to execute Large Language Models (LLMs) without requiring the entire model to be explicitly loaded into application-managed memory.

The primary objective is to implement **Intelligent Layer Streaming**, a memory-efficient inference architecture that loads transformer layers on demand, executes them sequentially, and releases or caches their weights according to memory availability and execution requirements.

Instead of relying on an external inference runtime such as Ollama, VirgoEngine will progressively implement its own:

- Model loading system.
- Model metadata parser.
- Tensor management.
- Transformer layer streaming.
- Intelligent prefetching.
- Memory allocation and management.
- Layer caching and eviction.
- Transformer inference.
- Text generation.
- CPU execution backend.

VirgoEngine is intended to become the native inference backend for the VirgoAI application.

It must remain an independent Rust library and must not depend on the VirgoAI desktop interface.

# 2. Primary Objective

The main engineering problem VirgoEngine addresses is model memory consumption.

Traditional inference runtimes may keep substantial portions of model weights resident in memory during inference.

This can create significant memory pressure, especially when running relatively large models on machines with limited unified memory.

VirgoEngine addresses this through intelligent weight streaming.

### Proposed approach

1. Keep the complete model weights stored on SSD.
2. Read model metadata without eagerly loading all tensor data.
3. Identify transformer layers and their associated tensors.
4. Load the weights required by the current execution stage.
5. Execute the current transformer layer.
6. Prefetch upcoming layers asynchronously.
7. Release previously used weights when no longer required and not worth caching.
8. Retain frequently reused weights when memory permits.
9. Repeat until the model completes its forward pass.

The engine must enforce a configurable application-managed memory budget.

**Important:** Transformer layers cannot normally be skipped arbitrarily. Every mathematically required operation must execute. The intelligence of this architecture is in memory scheduling, loading, caching, prefetching, and eviction.

# 3. Core Architectural Principles

All implementations must follow these principles.

### 3.1 Memory Efficiency

- Do not eagerly read the complete model into a single memory allocation.
- Keep model weights on persistent storage.
- Implement configurable memory budgets.
- Avoid unnecessary tensor copies.
- Release unused allocations.
- Manage KV cache separately from model weights.
- Account for prefetch buffers and intermediate activations.

### 3.2 Intelligent Layer Streaming

- Load transformer layers according to execution order.
- Prefetch upcoming layers.
- Track active tensor usage.
- Prevent eviction of tensors currently being used.
- Cache weights when beneficial.
- Support layer-level streaming before implementing tensor-level streaming.

### 3.3 Modular Architecture

Every subsystem must have a clearly defined responsibility.

The model loader must not execute transformer operations.

The memory manager must not understand model-specific attention calculations.

The inference engine must not depend on the desktop application.

The CPU backend must not depend on the model file format.

### 3.4 Hardware Independence

The inference architecture must separate computation from hardware-specific implementations.

The initial implementation targets CPU execution on Apple Silicon.

Future backends may include:

- ARM64 / NEON.
- Apple Metal.
- Other CPU architectures.
- Additional accelerator backends.

### 3.5 Correctness Before Optimization

The initial priority is mathematically correct inference and reliable memory management.

Do not introduce speculative optimizations before establishing correctness.

### 3.6 Explicit Resource Ownership

- Use safe Rust ownership wherever possible.
- Use explicit tensor lifetimes or lease-based ownership.
- Do not release memory while a computation is still using it.
- Track asynchronous I/O and execution dependencies.
- Ensure GPU resources remain alive until GPU operations complete.

# 4. Crate Structure

The crate must follow this directory structure:

```text
virgo_engine/
│
├── Cargo.toml
│
└── src/
    ├── lib.rs
    │
    ├── engine/
    │   ├── mod.rs
    │   ├── engine.rs
    │   └── config.rs
    │
    ├── model/
    │   ├── mod.rs
    │   ├── loader.rs
    │   ├── metadata.rs
    │   └── tensor.rs
    │
    ├── streaming/
    │   ├── mod.rs
    │   ├── layer_loader.rs
    │   ├── prefetch.rs
    │   ├── scheduler.rs
    │   └── cache.rs
    │
    ├── memory/
    │   ├── mod.rs
    │   ├── allocator.rs
    │   ├── budget.rs
    │   └── eviction.rs
    │
    ├── inference/
    │   ├── mod.rs
    │   ├── transformer.rs
    │   ├── attention.rs
    │   ├── feed_forward.rs
    │   └── generation.rs
    │
    └── backend/
        ├── mod.rs
        └── cpu.rs
```

Do not arbitrarily reorganize this structure.

If additional files become necessary, place them inside the appropriate existing subsystem and explain why they are required.

Do not create unnecessary modules or additional abstractions without a clear architectural purpose.

# 5. Module Responsibilities

## 5.1 lib.rs

The crate entry point.

Responsibilities:

- Declare public modules.
- Export the public API.
- Expose engine configuration.
- Expose supported model interfaces.
- Expose inference types.
- Expose public error types.

Keep implementation details inside their respective modules.

## 5.2 engine/

The main inference engine interface.

### engine.rs

Responsible for:

- Initializing the inference engine.
- Loading model metadata.
- Initializing the model source.
- Initializing the memory manager.
- Initializing the layer scheduler.
- Coordinating inference execution.
- Managing generation requests.
- Releasing runtime resources.

The engine must coordinate other subsystems instead of implementing their internal logic itself.

### config.rs

Defines configuration options.

Initial configuration should support:

- Model file path.
- Maximum application-managed memory budget.
- Maximum prefetch memory.
- Number of prefetched layers.
- Compute backend selection.
- Maximum context length.
- Execution settings.

Memory settings must be configurable instead of hardcoded.

## 5.3 model/

Responsible for model files, metadata, tensor indexing, and tensor access.

### loader.rs

Responsible for:

- Opening model files.
- Reading model metadata.
- Detecting supported model formats.
- Building tensor indexes.
- Locating tensor offsets.
- Providing access to tensor data.

Do not use `std::fs::read()` to load an entire large model file into a `Vec<u8>`.

Investigate memory-mapped file access through `memmap2` and bounded file reads.

Memory mapping must not be treated as a guarantee that the entire model consumes no physical RAM.

### metadata.rs

Stores model information, including:

- Architecture.
- Model name.
- Hidden dimension.
- Number of transformer layers.
- Attention heads.
- KV heads.
- Vocabulary size.
- Context length.
- Tensor metadata.
- Weight data types.

Metadata must be read from the actual model file.

Do not hardcode Qwen dimensions or tensor counts.

### tensor.rs

Defines the tensor abstraction.

Responsibilities:

- Tensor metadata.
- Tensor shape.
- Data type.
- Storage location.
- Byte offsets.
- Tensor size.
- Tensor access.
- Tensor ownership information.

Support multiple tensor representations.

Initial data types to investigate:

- F32.
- F16.
- BF16.
- Q8_0.
- Q4_0.
- Q4_K.

Only implement quantization formats when their actual binary representations and computation requirements are understood.

GGUF and Safetensors are storage formats, not interchangeable tensor execution systems.

## 5.4 streaming/

This is the most important subsystem of VirgoEngine.

Its responsibility is to implement Intelligent Layer Streaming.

### layer_loader.rs

Loads the tensors belonging to a requested transformer layer.

Requirements:

- Identify tensors associated with a layer.
- Retrieve tensor data from the model source.
- Allocate the required memory.
- Validate tensor sizes.
- Make loaded tensors available to inference.
- Track loading state.
- Report loading errors.

The loader must not load every model layer into memory during initialization.

### prefetch.rs

Responsible for predictive layer loading.

Example:

```text
Currently executing: Layer 08

Prefetch:
    Layer 09
    Layer 10

Cached:
    Layer 07
    Layer 08
    Layer 09
    Layer 10
```

Requirements:

- Predict upcoming layer requirements.
- Load upcoming weights asynchronously.
- Respect the memory budget.
- Prevent duplicate loading requests.
- Cancel unnecessary prefetch operations.
- Avoid unbounded queues.

Prefetching must never consume memory needed by active computation.

### scheduler.rs

Controls the layer execution sequence.

Responsibilities:

- Track current layer.
- Determine the next required layer.
- Coordinate layer loading.
- Coordinate prefetch operations.
- Coordinate execution readiness.
- Handle cancellation.
- Handle loading failures.

Layer execution order must follow the model architecture.

### cache.rs

Manages resident model weights.

Responsibilities:

- Store loaded layer weights.
- Track memory consumption.
- Track layer usage.
- Identify cache candidates.
- Prevent eviction of active tensors.
- Release unused buffers.
- Reuse previously loaded weights.

Use execution-aware caching.

Do not rely exclusively on ordinary LRU eviction because transformer execution follows a predictable sequence.

## 5.5 memory/

Responsible for application-managed memory allocation and resource management.

### allocator.rs

Handles tensor buffer allocation and release.

Requirements:

- Track allocated bytes.
- Track released bytes.
- Avoid unnecessary allocations.
- Support reusable buffers.
- Prevent invalid memory access.
- Avoid uncontrolled memory growth.

### budget.rs

Controls the memory limits.

Responsibilities:

- Define the maximum memory budget.
- Track current usage.
- Reserve memory before loading tensors.
- Account for prefetch allocations.
- Account for temporary execution buffers.
- Reject or delay allocations that exceed available budget.

The memory budget must be configurable.

Do not assume the entire physical memory of the machine is available to VirgoEngine.

### eviction.rs

Responsible for reclaiming memory.

Requirements:

- Identify unused tensors.
- Check active execution references.
- Respect tensor leases.
- Avoid releasing GPU resources prematurely.
- Coordinate with the cache manager.
- Reclaim memory when necessary.

Never evict a tensor that is actively being used by an execution operation.

## 5.6 inference/

Responsible for transformer computation and text generation.

### transformer.rs

Implements the transformer execution pipeline.

Responsibilities:

- Embedding operations.
- Transformer block execution.
- Normalization.
- Attention integration.
- Feed-forward integration.
- Output projection.

The transformer implementation must obtain weights through the streaming and model interfaces.

It must not directly load the complete model into memory.

### attention.rs

Responsible for attention computation.

Future requirements:

- Query projection.
- Key projection.
- Value projection.
- Rotary positional embeddings.
- Attention score calculation.
- Softmax.
- Attention output projection.
- Grouped-query attention support.
- KV cache integration.

The implementation must follow the actual model architecture.

### feed_forward.rs

Implements feed-forward operations.

For supported Qwen2 architectures, investigate the required SwiGLU-based feed-forward structure.

Requirements:

- Gate projection.
- Up projection.
- Activation.
- Down projection.
- Tensor operation integration.

### generation.rs

Responsible for autoregressive generation.

Requirements:

- Prompt processing.
- Prefill.
- Token-by-token decoding.
- Sampling.
- Temperature.
- Top-p.
- Maximum generated tokens.
- Stop conditions.
- Cancellation.
- Token streaming.

The generation subsystem must not directly depend on the VirgoAI desktop application.

## 5.7 backend/

Responsible for hardware-specific tensor execution.

### cpu.rs

Initial computation backend.

Requirements:

- Matrix multiplication.
- Vector operations.
- Normalization.
- Attention-related operations.
- Quantized tensor operations as supported.
- CPU parallelism.

Investigate ARM64 and NEON optimizations for Apple Silicon.

Keep the backend interface independent of model file formats.

Apple Metal acceleration is a future extension and should not be implemented prematurely.

# 6. Intelligent Layer Streaming Architecture

The intended execution architecture is:

```text
                MODEL FILE ON SSD
                       |
                       v
                MODEL LOADER
                       |
                       v
                TENSOR INDEX
                       |
                       v
                LAYER SCHEDULER
                       |
              +--------+--------+
              |                 |
              v                 v
         LAYER LOADER       PREFETCHER
              |                 |
              +--------+--------+
                       |
                       v
                MEMORY MANAGER
                       |
                       v
                 WEIGHT CACHE
                       |
                       v
              TRANSFORMER EXECUTION
                       |
                       v
                CPU BACKEND
                       |
                       v
               NEXT TRANSFORMER
                    LAYER
```

### Execution example

Assume the model contains 28 transformer layers.

```text
Layer 00:
    Load Layer 00
    Execute Layer 00
    Prefetch Layer 01

Layer 01:
    Execute Layer 01
    Prefetch Layer 02
    Evaluate Layer 00 for eviction

Layer 02:
    Execute Layer 02
    Prefetch Layer 03
    Evaluate Layer 01 for eviction

...

Layer 27:
    Execute Layer 27
    Release eligible cached weights

Generation:
    Produce output logits
    Sample next token
```

This sequence repeats during autoregressive generation.

Because weights are reused for every generated token, caching is important.

The engine should retain frequently reused weights when the available memory budget permits it.

It should not unnecessarily reload all layer weights from SSD for every generated token.

### Layer lifecycle

Each layer or tensor should have a clearly defined lifecycle:

```text
UNLOADED
    |
    v
REQUESTED
    |
    v
LOADING
    |
    v
RESIDENT
    |
    v
LEASED
    |
    v
EVICTABLE
    |
    v
UNLOADED
```

The implementation must prevent invalid state transitions and concurrent duplicate loads.

# 7. Memory Management Architecture

VirgoEngine must distinguish between different memory consumers.

| Memory category | Responsibility |
|---|---|
| Model weights | Transformer parameters |
| Activations | Intermediate computation results |
| KV cache | Attention keys and values |
| Workspace | Temporary computation buffers |
| Prefetch | Upcoming tensor data |

These resources must be accounted for independently.

The configured memory budget must not be exceeded by application-managed allocations.

Operating-system file cache and mapped file pages should be measured separately from explicitly managed allocations.

### KV Cache

KV cache is not the same as model weight storage.

It stores attention information from previously processed tokens.

It requires its own allocation, tracking, and memory-management strategy.

Do not implement KV cache as part of the layer weight cache.

### Memory safety

- No unsafe memory operations without documented justification.
- No use-after-free.
- No eviction during active execution.
- No uncontrolled allocation.
- No unbounded prefetching.
- No incorrect tensor offsets.
- No assumptions about available system memory.

# 8. Initial Model Compatibility

The initial target is Qwen2 / Qwen2.5.

However, the architecture must not be hardcoded exclusively for Qwen.

Model metadata should determine the architecture configuration.

Initial supported model format targets:

### GGUF

Investigate:

- Metadata parsing.
- Tensor indexing.
- Tensor offsets.
- Quantization metadata.
- Tensor data access.

### Safetensors

Investigate:

- Header parsing.
- Tensor metadata.
- Tensor offsets.
- Data types.
- Tensor data access.

The model source interface should allow both formats to provide tensor data to the same inference architecture.

Do not assume that every GGUF or Safetensors tensor is stored in the same data type.

Do not claim support for a format or quantization type until it has been implemented and tested.

# 9. Rust Dependencies

Evaluate dependencies based on their actual requirements.

Potential initial dependencies:

```toml
[dependencies]
memmap2 = "0.9"
safetensors = "0.6"
half = "2"
thiserror = "2"
tracing = "0.1"
```

Additional dependencies may be introduced when justified.

Potential future dependencies:

- Rayon for CPU parallelism.
- Tokio for asynchronous orchestration.
- Bytemuck for appropriate typed byte conversions.
- Specialized mathematical or quantized computation libraries.

Avoid unnecessary dependencies.

Do not introduce a complete external inference runtime as a replacement for VirgoEngine's own implementation.

Existing libraries may be used as references, validation tools, or carefully selected components.

# 10. Error Handling

Define meaningful errors for each subsystem.

Examples:

```text
ModelNotFound
UnsupportedModelFormat
UnsupportedArchitecture
InvalidModelMetadata
TensorNotFound
InvalidTensorOffset
UnsupportedTensorType
StorageReadFailed
MemoryBudgetExceeded
AllocationFailed
LayerLoadingFailed
PrefetchFailed
CacheOperationFailed
BackendUnavailable
KernelExecutionFailed
GenerationCancelled
ContextLimitExceeded
```

Errors must propagate correctly.

Do not silently ignore model loading failures or substitute incorrect tensor data.

Memory failures should trigger safe recovery when possible, such as eviction of eligible cached weights or reducing prefetch activity.

# 11. Development Roadmap

Implement the crate incrementally.

Do not attempt to implement all modules simultaneously.

## Phase 1 — Foundation

- Establish crate modules.
- Define public types.
- Define error handling.
- Define engine configuration.
- Define model metadata.
- Define tensor abstractions.
- Establish initial CPU backend interfaces.

Deliverable: A compilable and testable crate foundation.

## Phase 2 — Model Access

- Open model files.
- Implement GGUF metadata inspection.
- Implement Safetensors metadata inspection.
- Build tensor indexes.
- Identify transformer layers.
- Retrieve individual tensor data without eager full-model loading.

Deliverable: VirgoEngine can inspect and access model tensors.

## Phase 3 — Memory Management

- Implement allocator.
- Implement memory budget.
- Implement reservations.
- Implement tensor ownership.
- Implement cache.
- Implement safe eviction.

Deliverable: Controlled tensor memory management.

## Phase 4 — Layer Streaming

- Implement layer loader.
- Implement layer scheduler.
- Implement asynchronous prefetch.
- Implement cache integration.
- Implement memory-aware eviction.
- Implement layer lifecycle management.

Deliverable: Sequential layer loading and execution-ready tensor streaming.

## Phase 5 — Transformer Inference

- Implement embeddings.
- Implement normalization.
- Implement RoPE.
- Implement attention.
- Implement feed-forward.
- Implement transformer blocks.
- Implement logits and sampling.

Deliverable: Native text generation.

## Phase 6 — Optimization

- Optimize CPU execution.
- Introduce ARM64 SIMD.
- Improve prefetch scheduling.
- Optimize cache behavior.
- Optimize KV cache.
- Investigate Metal acceleration.

Deliverable: More efficient native inference.

# 12. Testing Requirements

Every subsystem must have tests.

### Model tests

- Valid model metadata.
- Invalid model metadata.
- Tensor indexing.
- Tensor offset validation.
- Supported and unsupported data types.

### Streaming tests

- Correct layer order.
- Layer loading.
- Prefetch scheduling.
- Duplicate request prevention.
- Cache reuse.
- Cache eviction.
- Cancellation.

### Memory tests

- Allocation tracking.
- Memory budget enforcement.
- Reservation failures.
- Safe eviction.
- Active tensor protection.
- Memory reclamation.

### Inference tests

- Individual mathematical operations.
- Transformer layer execution.
- Attention correctness.
- Feed-forward correctness.
- Token generation.
- Comparison against a trusted reference implementation.

### Stress tests

- Repeated inference.
- Large model files.
- Low memory budgets.
- Long context lengths.
- Repeated model loading and unloading.
- Concurrent loading and execution.

# 13. Performance Metrics

VirgoEngine must expose useful metrics.

Track:

- Total model file size.
- Explicitly allocated weight memory.
- Peak application-managed memory.
- Current memory usage.
- KV cache consumption.
- Activation memory.
- Prefetch memory.
- Layer loading duration.
- SSD read throughput.
- Cache hit rate.
- Cache eviction count.
- Prefetch effectiveness.
- Time to first token.
- Tokens per second.

Distinguish application-managed allocations from operating-system-managed file cache.

The objective is to determine whether layer streaming genuinely reduces physical memory pressure without introducing unacceptable inference latency.

# 14. Integration Boundaries

VirgoEngine is an independent library.

It must not depend on:

- Iced.
- VirgoAI desktop components.
- Chat interface implementation.
- Project UI.
- Terminal UI.
- Existing Ollama runtime internals.

The VirgoAI application should eventually communicate with VirgoEngine through a clean public API.

The existing Ollama integration may remain available as an optional external inference provider during development.

Do not remove, replace, or modify the existing Ollama crate as part of implementing this crate unless explicitly requested.

# 15. Rules for Claude Code

When working on VirgoEngine, follow these instructions strictly.

1. Inspect the existing VirgoEngine source tree before making changes.
2. Read the existing Cargo.toml and root workspace configuration.
3. Preserve the established directory structure.
4. Do not rewrite unrelated VirgoAI crates.
5. Do not modify the desktop application.
6. Do not introduce Ollama as an internal dependency.
7. Do not load the entire model into a single Vec or equivalent eager allocation.
8. Do not pretend memory mapping automatically solves physical memory consumption.
9. Do not implement fake tensor operations and describe them as real inference.
10. Do not skip required transformer operations.
11. Do not implement unverified quantization formats.
12. Do not hardcode model architecture dimensions.
13. Do not implement GPU acceleration before a correct CPU execution path exists.
14. Avoid unnecessary unsafe Rust.
15. Write tests alongside implementations.
16. Preserve existing functionality.
17. Run cargo check and relevant tests after implementation.
18. Document architectural decisions and limitations.

### Implementation workflow

Before implementing a subsystem:

1. Inspect the relevant files.
2. Identify existing implementations.
3. Explain the intended changes.
4. Implement the smallest coherent unit.
5. Add tests.
6. Run the relevant checks.
7. Report completed work and remaining limitations.

Never claim that a subsystem is complete if it only contains placeholder structures or unimplemented methods.

# 16. Definition of Success

VirgoEngine's initial architecture is successful when it can:

- Open a supported model file.
- Read metadata without eagerly loading all model weights.
- Identify transformer layers.
- Index tensor locations.
- Load individual layer weights.
- Prefetch upcoming layers.
- Manage resident weights within a configurable application-managed memory budget.
- Safely release unused tensors.
- Prevent eviction of actively used tensors.
- Report memory and streaming statistics.

The next milestone is to connect this streaming infrastructure to actual transformer computations and achieve correct native text generation.

---

# Final Project Objective

VirgoEngine is intended to become a native Rust inference engine that makes local LLM execution more memory-efficient through intelligent model weight management.

Its defining capability is **Intelligent Layer Streaming**:

Load only the weights needed for the current execution stage, predictively prepare upcoming layers, execute transformer operations, and reclaim or cache memory according to actual usage and available resources.

The engine must prioritize correctness, memory safety, predictable resource consumption, modularity, and measurable performance.

**Build a real inference engine, not an Ollama wrapper.**