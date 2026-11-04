# StateSolver — Research Direction & Project Brief

## 1. Problem

Modern reversible debuggers can replay a failed execution and move backward in time, while dynamic slicing tools can identify instructions/statements that influenced a value.

However, a difficult class of native C/C++ bugs remains awkward to explain: **temporal memory bugs where the same virtual address represents different heap objects at different times**.

Example:

```text
malloc() -> object A at 0x55550000
free(A)
malloc() -> object B reuses 0x55550000
stale pointer to A writes into 0x55550000
program crashes much later
```

A debugger that reasons only about addresses may see the same location throughout the trace, even though its **object identity changed across allocation lifetimes**.

---

## 2. Scrutiny of Existing Work

This project must **not** claim the following as novel by themselves:

- reverse execution / time-travel debugging;
- deterministic record and replay;
- dynamic program slicing;
- dynamic dependence graphs;
- finding the previous write to a variable;
- tracing values back to earlier sources;
- detecting use-after-free.

These already appear in systems and research such as:

- **Omniscient Debugging** — records state changes and supports backward navigation.
- **rr** — practical Linux record/replay and reverse execution.
- **Dynamic Program Slicing** — Agrawal & Horgan (PLDI 1990).
- **SPYDER** — combines dynamic slicing and backtracking for debugging.
- **DrDebug** — combines deterministic replay with dynamic slicing.
- **Pernosco** — provides time-oriented debugging and value-flow tracing over rr recordings.
- Existing UAF research and tooling can identify frees, invalid accesses, and candidate root causes.

Therefore, simply building:

```text
statesolver why rax
statesolver why memory[address]
```

would be a strong engineering project, but **not a defensible research novelty claim**.

---

## 3. Research Gap

The proposed gap is the combination of:

1. **binary-level debugging of unmodified native programs**;
2. **deterministic replay**;
3. **on-demand causal reconstruction**, rather than maintaining a complete dependence graph for the entire execution;
4. **allocation-epoch-aware memory identity** so that:
   ```text
   address + lifetime
   ```
   rather than only:
   ```text
   address
   ```
   identifies a heap object;
5. causal explanation that can cross:
   - register transfers,
   - memory loads/stores,
   - pointer copies,
   - allocation/free/reallocation boundaries,
   - address reuse,
   - system-call supplied input.

The important distinction is:

```text
0x55550000 @ allocation_epoch_17
```

is treated as a different logical object from:

```text
0x55550000 @ allocation_epoch_18
```

even though the virtual address is identical.

---

# 4. Proposed Research Question

> **Can deterministic replay be used to reconstruct, on demand, allocation-epoch-aware causal slices for arbitrary machine-state values in native binaries, while avoiding the storage and runtime cost of maintaining a complete dynamic dependence graph during the original execution?**

### Secondary question

> **Does distinguishing heap objects by allocation lifetime significantly improve root-cause localization for temporal memory bugs involving address reuse, compared with address-only backward tracing?**

This is the research question the project should actually test.

It is substantially narrower and more defensible than simply claiming "causal debugging" or "time-travel debugging" as novel.

---

## 5. Core Idea — StateSolver

StateSolver is a reversible Linux debugger for x86-64 native programs.

It records an execution and lets the developer inspect:

```bash
statesolver run ./program
statesolver timeline
statesolver regs
statesolver memory
statesolver allocations
statesolver rewind <event>
```

The research layer adds:

```bash
statesolver why <register>
statesolver why <address>
statesolver origin <pointer>
statesolver lifetime <address>
statesolver explain-crash
```

Instead of returning only the last writer, StateSolver attempts to construct a compact causal chain.

Example:

```text
SIGSEGV
  ↓
invalid pointer dereference
  ↓
pointer copied at event 81,219
  ↓
points to Allocation #41 / Epoch #3
  ↓
Allocation #41 freed at event 73,882
  ↓
same virtual address reused by Allocation #79
  ↓
stale pointer from Epoch #3 accessed Epoch #4 memory
```

---

## 6. Proposed Architecture

```text
                Native Linux Program
                        |
                        v
                 Execution Recorder
                        |
          +-------------+-------------+
          |                           |
          v                           v
 deterministic trace          allocation events
          |                   malloc/free/mmap...
          +-------------+-------------+
                        |
                        v
                    Replay Engine
                        |
                        v
              Query-Driven Instrumentation
                        |
         +--------------+--------------+
         |              |              |
         v              v              v
    registers       memory flow    object lifetime
         \              |              /
          \             |             /
           +------------+------------+
                        |
                        v
               Temporal Object Graph
                        |
                        v
                 Causal Slice
```

---

## 7. Key Data Model

Heap identity should not be represented only by an address.

Use:

```text
ObjectID = {
    address_range,
    allocation_event,
    free_event,
    generation / epoch
}
```

Pointer provenance can therefore reference:

```text
Pointer {
    value: 0x55550000,
    object: Allocation_41,
    epoch: 3
}
```

If the address is reused:

```text
Allocation_79
address = 0x55550000
epoch = 4
```

StateSolver knows these are separate objects.

---

## 8. Novel Mechanism to Investigate

Instead of constructing a complete Dynamic Dependence Graph during program execution, StateSolver can attempt **query-driven reconstruction**.

Example:

```bash
statesolver why rax@event_900000
```

The system:

1. restores a nearby checkpoint;
2. deterministically replays the relevant interval;
3. instruments only the values required by the query;
4. discovers predecessor dependencies;
5. recursively follows only relevant predecessors;
6. stops at external input, constants, allocations, or user-selected boundaries.

Conceptually:

```text
Crash
  |
  v
target value
  |
  +--> replay relevant interval
          |
          +--> discover producer
                    |
                    +--> discover producer's inputs
                              |
                              +--> repeat
```

The hypothesis is that **causal information can be generated lazily**, reducing persistent trace size compared with eagerly storing full machine-level dependency information.

---

## 9. Evaluation

Create controlled C/C++ workloads containing:

- use-after-free;
- address reuse after free;
- stale-pointer writes;
- double free;
- heap-buffer overwrite;
- incorrect pointer propagation;
- corrupted function pointer;
- external-input-induced corruption.

Compare at minimum:

### A. Address-only tracing

```text
memory address -> previous write
```

### B. Allocation-epoch-aware StateSolver

```text
memory address
+ allocation identity
+ lifetime
+ data dependency
+ temporal dependency
```

Measure:

```text
runtime overhead during recording
trace size
query reconstruction time
number of candidate causal events
causal-slice size
distance from crash to true root cause
root-cause localization accuracy
```

A particularly useful metric:

```text
Root Cause Reduction Ratio =
causally relevant events / total events examined
```

---

## 10. Hypothesis

> Allocation-epoch-aware causal reconstruction will substantially reduce ambiguous or incorrect causal chains in executions containing heap address reuse, while query-driven replay can recover useful machine-state provenance without continuously recording a complete dynamic dependence graph.

---

## 11. Research Claim — Keep It Narrow

Do **not** claim:

> "First debugger that explains why a value exists."

That is too broad and likely false.

A safer target claim, if experiments support it, would be:

> **A query-driven reversible-debugging technique that reconstructs machine-level causal slices using allocation-lifetime identity, enabling temporal heap dependencies to be distinguished across virtual-address reuse without eagerly retaining a complete dynamic dependence graph.**

Whether this exact combination is publishably novel still needs a formal systematic literature review before submission, but the search performed for this project did not reveal an existing system with this exact scope and mechanism.

---

## 12. MVP Scope

Start deliberately narrow:

```text
Platform:      Linux
Architecture:  x86-64
Programs:      single-process
Threads:       single-threaded initially
Language:      C/C++
Binary format: ELF
Debug info:    DWARF optional
```

### Phase 1

- launch/attach using `ptrace`;
- register inspection;
- `/proc/<pid>/maps`;
- ELF parsing;
- instruction stepping;
- event timeline.

### Phase 2

- deterministic replay;
- checkpoints;
- reverse-step / rewind.

### Phase 3

- intercept malloc/free/realloc/mmap/munmap;
- assign allocation IDs and epochs;
- reconstruct allocation lifetimes.

### Phase 4

- track register and memory dependencies during targeted replay;
- implement:
  ```bash
  statesolver why
  statesolver origin
  statesolver lifetime
  ```

### Phase 5

- causal slicing;
- automatic crash explanation;
- experimental evaluation.

---

## 13. Short Project Pitch

**StateSolver** is a reversible Linux debugger that treats runtime debugging as causal reconstruction rather than simple backward execution.

It records a native program once and allows developers to travel through register values, memory mutations, allocations and object lifetimes. Its research component investigates whether machine-state provenance can be reconstructed **on demand through deterministic replay**, while treating each heap allocation lifetime as a unique temporal object even when virtual addresses are reused.

The final goal is:

```bash
statesolver explain-crash
```

producing something like:

```text
Crash: invalid read at 0x55550020

Cause:
stale pointer created at event 18,201
        |
        v
Allocation #31 (epoch 7)
freed at event 41,882
        |
        v
address reused by Allocation #64 (epoch 8)
        |
        v
old pointer used at event 91,122
        |
        v
memory corruption
        |
        v
SIGSEGV
```

In short:

> **not merely "go backward in time", but reconstruct which historical object and state transition caused the current machine state.**

---

## 14. Relevant Prior Work

- Bil Lewis, **Debugging Backwards in Time / Omniscient Debugging**  
  https://arxiv.org/abs/cs/0310016

- Robert O'Callahan et al., **Engineering Record And Replay For Deployability (rr)**  
  https://arxiv.org/abs/1705.05937

- Hiralal Agrawal and Joseph Horgan, **Dynamic Program Slicing**, PLDI 1990  
  https://doi.org/10.1145/93542.93576

- Agrawal, DeMillo, Spafford, **Debugging with Dynamic Slicing and Backtracking**, 1993  
  https://doi.org/10.1002/spe.4380230603

- Wang et al., **DrDebug: Deterministic Replay Based Cyclic Debugging with Dynamic Slicing**, CGO 2014  
  https://doi.org/10.1145/2544137.2544152

- **Pernosco** — modern time-oriented debugger built on rr  
  https://pernos.co/about/

- Yang Wu, Ang Chen, Linh Thi Xuan Phan, **Zeno: Diagnosing Performance Problems with Temporal Provenance**, NSDI 2019  
  https://www.usenix.org/conference/nsdi19/presentation/wu


---

# 15. Two-Stage System Architecture

StateSolver should be designed as two major implementation stages with a strict boundary between low-level execution capture and user-facing analysis.

The guiding rule is:

```text
collect facts first
        ↓
reconstruct and analyze them
        ↓
visualize them
```

The UI must never be responsible for tracing the target process directly.

---

# Stage 1 — Backend: Trace Collection, Replay & State Reconstruction

## Objective

Build the low-level systems layer responsible for observing a native Linux process and producing a deterministic, queryable execution trace.

This stage is the technical foundation of the entire project and should work without any graphical interface.

The target experience is:

```bash
statesolver record ./program
```

which produces a trace:

```text
program.ss
```

that can later be queried or opened by the UI.

---

## Stage 1 Responsibilities

### 1. Process Control

Launch or attach to Linux processes using mechanisms such as:

```text
ptrace
waitpid
signals
/proc/<pid>/*
```

Initial target:

```text
Linux
x86-64
ELF binaries
single process
single thread
```

Multithreading should be added only after deterministic single-thread replay is stable.

---

### 2. Register State Collection

Collect CPU state at relevant execution points.

Example:

```text
RAX
RBX
RCX
RDX
RSI
RDI
RSP
RBP
RIP
R8-R15
RFLAGS
```

Events should support register deltas:

```text
event #18442

RAX:
0x00000020
    ↓
0x55556120

RIP:
0x401284
    ↓
0x401289
```

The trace should avoid storing full register snapshots unnecessarily when a compact delta representation is sufficient.

---

### 3. Memory Mapping Tracking

Track the process virtual address space through:

```text
/proc/<pid>/maps
mmap
munmap
mprotect
brk
```

Represent regions such as:

```text
.text
.data
.bss
heap
stack
shared libraries
anonymous mappings
```

Example:

```text
0x00400000 - 0x00412000   executable
0x55556000 - 0x55598000   heap
0x7ffff7...                libc
0x7ffffff...               stack
```

StateSolver should know when mappings appear, disappear, expand, shrink, or change permissions.

---

### 4. Heap Allocation Lifetime Tracking

Track:

```text
malloc
calloc
realloc
free
mmap-backed allocations
```

Each allocation receives a logical identity.

Do not identify a heap object only using its address.

Use:

```text
Allocation {
    id,
    address,
    size,
    allocation_event,
    free_event,
    epoch
}
```

Example:

```text
Allocation #41
address: 0x55550000
epoch: 7
```

After free and reuse:

```text
Allocation #79
address: 0x55550000
epoch: 8
```

These must be treated as different objects even though they share the same virtual address.

This is central to the research contribution.

---

### 5. System Call Recording

Record nondeterministic interaction with the operating system.

Important categories include:

```text
read
write
open
close
mmap
munmap
brk
clock/time calls
random input
signals
network I/O
file I/O
```

For deterministic replay, the recorder must capture enough syscall result information to reproduce observable program behavior.

---

### 6. Execution Timeline

Every recorded event should have a globally ordered identifier:

```text
E1
E2
E3
...
E18442
```

Example:

```text
E1200  malloc(1024)
E1201  return 0x55550000
E1310  memory write
E1810  free(0x55550000)
E1902  malloc(512)
E1903  return 0x55550000
E4221  stale pointer write
E4300  SIGSEGV
```

This timeline becomes the backbone for rewind, replay, provenance, and visualization.

---

### 7. Checkpoint System

StateSolver should periodically checkpoint execution.

A checkpoint may contain:

```text
register state
memory mappings
selected process memory
allocation metadata
replay metadata
```

Execution between checkpoints should be represented through deltas and deterministic replay.

Conceptually:

```text
Checkpoint A
     |
     +-- E1001
     +-- E1002
     +-- E1003
     |
Checkpoint B
```

This avoids full-process snapshots at every event.

---

### 8. Deterministic Replay Engine

The replay system should restore a checkpoint and reproduce execution until a selected event.

Required operations:

```bash
statesolver rewind 18442
statesolver replay 18000..18442
statesolver state --event 18442
```

The replay engine is also what enables later query-driven causal reconstruction.

---

### 9. Trace Storage Format

The backend should output a stable trace format independent of the UI.

Conceptual event:

```json
{
  "event": 18442,
  "type": "memory_write",
  "thread": 4912,
  "rip": "0x401289",
  "address": "0x55556120",
  "size": 8,
  "allocation_id": 41,
  "epoch": 3
}
```

JSON can be used during prototyping.

The production trace should eventually use a compact binary representation because traces may contain millions of events.

Possible later choices:

```text
custom binary format
Protocol Buffers
FlatBuffers
Cap'n Proto
```

The format should be versioned.

---

## Stage 1 Suggested Internal Modules

```text
backend/

├── tracer/
│   ├── process_control/
│   ├── ptrace/
│   ├── registers/
│   ├── syscalls/
│   ├── signals/
│   └── memory/
│
├── elf/
│   ├── loader_info/
│   ├── symbols/
│   └── dwarf/
│
├── allocations/
│   ├── hooks/
│   ├── lifetime_tracker/
│   └── epochs/
│
├── recorder/
│   ├── event_log/
│   ├── checkpoints/
│   └── trace_writer/
│
├── replay/
│   ├── checkpoint_restore/
│   ├── syscall_replay/
│   └── deterministic_runner/
│
└── trace_format/
```

---

# Stage 2 — Analysis Engine & User Interface

## Objective

Turn the raw execution trace into understandable debugging information.

Stage 2 should answer questions such as:

```text
What was the state here?

What wrote this memory?

Where did this pointer come from?

Which allocation does this address belong to?

Was this address reused?

Why does this register contain this value?

What chain of events caused the crash?
```

The UI is only one consumer of this layer.

The analysis engine should also work through CLI or APIs.

---

## Stage 2A — Analysis Engine

### 1. Historical State Queries

Provide queries such as:

```bash
statesolver regs --event 18442
statesolver memory 0x55550000 --event 18442
statesolver maps --event 18442
statesolver stack --event 18442
```

---

### 2. Register History

Example:

```bash
statesolver history rax
```

Output:

```text
E18200  0x00000000
E18242  0x00000400
E18376  0x55550000
E18442  0x55556120
```

The debugger should eventually identify the instruction responsible for each change.

---

### 3. Memory History

Example:

```bash
statesolver history 0x55556120
```

Output:

```text
E1201   allocation #41 created
E1310   00 -> 2A
E1829   2A -> 41
E1810   allocation #41 freed
E1903   allocation #79 reused address
E4221   stale write detected
```

---

### 4. Allocation Lifetime Queries

Example:

```bash
statesolver lifetime 0x55550000
```

Output:

```text
Address 0x55550000

Allocation #41
epoch: 7
created: E1200
freed:   E1810

Allocation #79
epoch: 8
created: E1902
freed:   E5200
```

This allows the debugger to distinguish address reuse.

---

### 5. Query-Driven Causal Reconstruction

This is the core research mechanism.

Example:

```bash
statesolver why rax --event 18442
```

Instead of constructing and storing a complete dependence graph during the original recording, StateSolver should attempt to reconstruct only the dependencies needed for the query.

Possible algorithm:

```text
target state
    |
    v
find producing instruction
    |
    v
identify input registers / memory
    |
    v
replay relevant region with instrumentation
    |
    v
find predecessor state
    |
    v
repeat recursively
```

Stop conditions may include:

```text
constant
program input
system call result
allocation event
function argument
user-selected boundary
```

---

### 6. Temporal Object Graph

Causal reasoning should operate on logical memory objects rather than raw addresses where possible.

Example:

```text
Allocation #41 / Epoch 7
         |
         v
pointer copied into RBX
         |
         v
stored inside Session.cached_user
         |
         v
Allocation #41 freed
         |
         v
address reused by Allocation #79 / Epoch 8
         |
         v
old Session.cached_user dereferenced
         |
         v
memory corruption
```

This graph is one of StateSolver's defining features.

---

### 7. Crash Explainer

Final command:

```bash
statesolver explain-crash
```

Example output:

```text
SIGSEGV at auth.c:184

Immediate failure:
invalid dereference of 0x55550020

Pointer origin:
Allocation #31 / epoch 7

Allocation created:
E18291
load_user()

Allocation freed:
E41882
logout()

Address reused:
E44110
Allocation #64 / epoch 8

Stale pointer retained in:
session->cached_user

Invalid dereference:
E91122

Likely classification:
use-after-free with address reuse
```

---

# Stage 2B — Visualization UI

## UI Goal

The UI should behave like a **time machine for a process**, not simply a graphical GDB frontend.

Changing the selected event should update all visible state simultaneously.

---

## Main UI Layout

Suggested layout:

```text
+------------------------------------------------------------+
| StateSolver                     event 18442 / 91821        |
+------------------------------------------------------------+
| Timeline                                                   |
|                                                            |
| malloc --- write --- free --- reuse -------- CRASH         |
|   A41       |        |      A79                ^           |
|             +----------------------------------+           |
+-------------------------+----------------------------------+
| Registers               | Memory                           |
|                         |                                  |
| RAX 0x55556120          | 0x55556120  41 41 41 41        |
| RBX 0x00000008          | 0x55556128  00 00 00 00        |
| RIP 0x401289            |                                  |
| RSP 0x7fffffff...       | Allocation #41                  |
|                         | Epoch 3                          |
+-------------------------+----------------------------------+
| Causal Graph                                               |
|                                                            |
| input                                                      |
|   ↓                                                        |
| parse_length()                                             |
|   ↓                                                        |
| RAX                                                        |
|   ↓                                                        |
| heap write                                                 |
|   ↓                                                        |
| stale pointer                                              |
|   ↓                                                        |
| crash                                                      |
+------------------------------------------------------------+
```

---

## UI Components

### Timeline

Show:

```text
function calls
allocations
frees
syscalls
signals
memory writes
checkpoints
crashes
```

Allow:

```text
zoom
filter
jump to event
drag backward/forward
```

---

### Register Inspector

Display:

```text
current value
previous value
change event
producer instruction
history
```

Clicking a register should allow:

```text
Why does RAX contain this?
Show history
Jump to producer
```

---

### Memory Inspector

Features:

```text
hex view
ASCII view
changed bytes
allocation ownership
memory permissions
last writer
history
```

Allocation epochs should be visually distinct.

---

### Allocation View

Show allocation lifetimes as ranges over time.

Example:

```text
Allocation #41
E1200 ================= E1810

Allocation #79
                  E1902 ======================== E5200
```

If both occupy the same virtual address, the UI should make the generation change obvious.

---

### Stack View

Show:

```text
call stack
stack frames
return addresses
local variables where DWARF is available
```

Dragging the timeline should reconstruct the historical stack.

---

### Source / Disassembly View

Display:

```text
source code when debug symbols exist
assembly otherwise
```

Highlight:

```text
current RIP
producer instruction
memory access
causal predecessor
```

---

### Causal Graph View

Visualize the output of:

```bash
statesolver why ...
```

Example:

```text
read(fd)
   |
   v
buffer[12]
   |
   v
RAX
   |
   v
length
   |
   v
memcpy
   |
   v
Allocation #41 corruption
   |
   v
crash
```

Nodes should be clickable and jump the global timeline to their event.

---

# Backend ↔ Frontend Boundary

The frontend must never call `ptrace` or inspect `/proc` itself.

Use:

```text
Target Program
      |
      v
Backend Recorder
      |
      v
Trace File
      |
      v
Analysis Engine
      |
      +------ CLI
      |
      +------ UI
      |
      +------ future IDE extension
```

This separation allows traces to be recorded on:

```text
servers
CI runners
Raspberry Pi
remote development systems
headless Linux machines
```

and analyzed somewhere else.

---

## Analysis API

The analysis layer should expose stable operations such as:

```text
get_event(id)

get_register_state(event)

get_memory(address, event)

get_allocation(address, event)

get_allocation_history(address)

get_register_history(register)

get_memory_history(address)

rewind(event)

find_last_writer(address, event)

find_value_origin(target, event)

build_causal_slice(target, event)

explain_crash()
```

The CLI and UI should both consume this same interface.

---

# 16. Recommended Repository Structure

```text
statesolver/

├── backend/
│   ├── tracer/
│   ├── elf/
│   ├── memory/
│   ├── allocations/
│   ├── recorder/
│   ├── checkpoints/
│   └── replay/
│
├── analysis/
│   ├── state_query/
│   ├── provenance/
│   ├── slicing/
│   ├── temporal_objects/
│   └── crash_explainer/
│
├── trace-format/
│
├── cli/
│
├── ui/
│   ├── timeline/
│   ├── registers/
│   ├── memory/
│   ├── allocations/
│   ├── stack/
│   ├── disassembly/
│   └── causal_graph/
│
├── benchmarks/
│
├── bug-corpus/
│
├── tests/
│
└── docs/
```

---

# 17. Development Order

Do not start with the graphical UI.

Recommended order:

```text
1. Process tracing
      ↓
2. Registers + instruction stepping
      ↓
3. Memory maps
      ↓
4. Event trace format
      ↓
5. Allocation lifetime tracking
      ↓
6. Checkpointing
      ↓
7. Deterministic replay
      ↓
8. CLI historical queries
      ↓
9. Query-driven provenance
      ↓
10. Allocation-epoch causal reconstruction
      ↓
11. Crash explainer
      ↓
12. Visual UI
```

A minimal UI may be created earlier for debugging the backend, but it should not dictate backend architecture.

---

# 18. Definition of the Two Deliverables

## Deliverable A — StateSolver Core

A headless low-level debugger capable of:

```bash
statesolver record
statesolver replay
statesolver rewind
statesolver regs
statesolver memory
statesolver allocations
statesolver history
statesolver why
statesolver explain-crash
```

This is the main engineering and research component.

---

## Deliverable B — StateSolver Visualizer

A desktop/web-based analysis environment capable of loading a `.ss` trace and visually exploring:

```text
execution timeline
register history
memory state
heap allocations
allocation epochs
stack state
source/disassembly
causal dependencies
crash explanations
```

The frontend is therefore an interactive visualization of the backend's reconstructed process history rather than a separate debugger implementation.

---

# 19. Final Architecture Summary

```text
                 STAGE 1
        LOW-LEVEL SYSTEMS BACKEND

             Target Process
                   |
                   v
         Trace / Replay Engine
                   |
       +-----------+------------+
       |           |            |
    registers    memory     allocations
       |           |            |
       +-----------+------------+
                   |
              .ss trace
                   |
                   v

                 STAGE 2
         ANALYSIS + VISUALIZATION

             Analysis Engine
                   |
       +-----------+------------+
       |           |            |
    history      why()     explain-crash
       |           |            |
       +-----------+------------+
                   |
           Temporal Object Graph
                   |
          +--------+---------+
          |                  |
         CLI                 UI
```

The research paper should focus primarily on the boundary between these stages:

> **how deterministic execution data from Stage 1 can be transformed by Stage 2 into allocation-epoch-aware causal explanations without requiring a complete dependence graph to be recorded eagerly.**


---

# 20. Stage 4 — Interactive Debugging Harness

## Objective

Build a programmable interaction layer on top of StateSolver so that developers, scripts, IDEs, and AI agents can investigate an execution directly without manually operating the low-level tracing and analysis components.

The full system becomes:

```text
Program
   |
   v
[1] Trace Collection
   |
   v
Execution Trace
   |
   v
[2] Replay + StateSolver Analysis
   |
   v
Causal / Historical State Model
   |
   +----------------------+
   |                      |
   v                      v
[3] Visual UI       [4] Interaction Harness
                          |
                          +--> CLI
                          +--> API
                          +--> Agent
                          +--> IDE
                          +--> automation
```

The harness must not perform low-level tracing itself.

It should communicate with the StateSolver analysis engine through a stable query and action interface.

---

## Why the Harness Exists

Without the harness, a developer may manually perform:

```text
find crash
    ↓
inspect register
    ↓
find pointer
    ↓
search memory history
    ↓
identify allocation
    ↓
inspect free event
    ↓
trace pointer backwards
```

The harness allows that investigation to become:

```bash
statesolver ask "Why did this program crash?"
```

or programmatically:

```text
explain_crash()
    ↓
inspect_pointer()
    ↓
get_allocation_history()
    ↓
build_causal_slice()
    ↓
return explanation
```

This turns StateSolver from a passive debugger into an **interactive debugging platform**.

---

# Harness Responsibilities

## 1. Unified StateSolver API

Expose high-level primitives such as:

```text
get_current_state()

get_event(event_id)

get_register(register, event_id)

get_register_history(register)

read_memory(address, size, event_id)

get_memory_history(address)

get_allocation(address, event_id)

get_allocation_history(address)

find_last_writer(address, event_id)

find_pointer_origin(pointer, event_id)

build_causal_slice(target, event_id)

rewind(event_id)

search_events(filter)

explain_crash()
```

These operations should be stable regardless of whether they are called by:

```text
CLI
UI
IDE
script
AI agent
```

---

## 2. Investigation Sessions

The harness should maintain an investigation context.

Example:

```text
Session:
target trace: crash.ss
current event: E91821
selected thread: T3
selected allocation: A41
selected address: 0x55556120
active hypothesis: possible use-after-free
```

This allows follow-up interactions such as:

```text
> Why did it crash?

> Where did that pointer come from?

> When was it freed?

> Show me the write before that.

> Rewind there.

> What changed between those two events?
```

The user or agent should not need to repeat every address and event ID.

---

## 3. Structured Tool Interface

The harness should expose machine-readable operations.

Conceptual request:

```json
{
  "tool": "find_pointer_origin",
  "pointer": "0x55550020",
  "event": 91821
}
```

Conceptual response:

```json
{
  "allocation_id": 31,
  "epoch": 7,
  "created_event": 18291,
  "freed_event": 41882,
  "address_reused": true,
  "replacement_allocation": 64
}
```

The exact transport can evolve independently of the underlying debugger.

Possible interfaces:

```text
local IPC
Unix domain socket
stdio protocol
JSON-RPC
gRPC
MCP-compatible server
```

The first implementation should prefer the simplest reliable local protocol.

---

## 4. Agent Harness

An AI or autonomous debugging agent should operate only through explicit StateSolver tools.

Example tool set:

```text
trace.summary
trace.search_events

state.registers
state.memory
state.stack

history.register
history.memory

allocation.inspect
allocation.history

replay.goto
replay.range

causal.origin
causal.slice

crash.explain
```

An agent investigation could therefore perform:

```text
crash.explain
      |
      v
identify bad pointer
      |
      v
allocation.history
      |
      v
detect freed allocation
      |
      v
causal.origin(pointer)
      |
      v
find stale pointer propagation
      |
      v
causal.slice
      |
      v
produce root-cause explanation
```

The agent is **not** trusted to invent runtime facts.

Every factual claim should originate from a StateSolver query result.

---

## 5. Evidence-Backed Answers

The harness should make every explanation traceable to concrete execution evidence.

Example:

```text
Likely root cause:
Use-after-free involving Allocation #31.

Evidence:

E18291
malloc(128) -> 0x55550000

E41882
free(0x55550000)

E44110
malloc(256) -> 0x55550000
address reused as Allocation #64

E91122
stale pointer from Allocation #31 dereferenced

E91821
SIGSEGV
```

This property is particularly important when an LLM is attached to the harness.

The language model explains the evidence.

It does not generate the evidence.

---

## 6. Natural-Language Interaction

Once the structured harness is stable, an optional natural-language layer can translate questions such as:

```text
"Why did this crash?"

"What changed in this object before it broke?"

"Where did this value in RAX originate?"

"Was this pointer already freed?"

"Show me everything that influenced this branch."

"What happened to this address between allocation and crash?"

"Compare memory before and after this function."

"Take me to the first suspicious event."
```

into deterministic StateSolver tool calls.

Example:

```text
User:
Why is RAX 0x55550020 here?

        |
        v

Harness Planner

        |
        +--> get_register(RAX, E91800)
        +--> find_value_origin(RAX, E91800)
        +--> get_allocation(0x55550020, E91800)
        +--> get_allocation_history(...)
        +--> build_causal_slice(...)

        |
        v

Evidence-backed explanation
```

---

## 7. Guardrails

The interaction layer should distinguish between:

```text
FACT
derived directly from execution evidence

INFERENCE
produced by the analysis engine

HYPOTHESIS
suggested by an agent or model
```

Example:

```text
FACT:
Allocation #41 was freed at E1802.

FACT:
0x55550000 was reused by Allocation #79 at E1902.

INFERENCE:
The dereference at E4201 refers to an obsolete allocation epoch.

HYPOTHESIS:
This stale pointer was probably retained in session->user.
```

Hypotheses should be explicitly verified with additional StateSolver queries before being presented as root causes.

---

# Harness CLI Example

The first version can remain entirely terminal-based.

```bash
statesolver session crash.ss
```

Then:

```text
StateSolver > explain crash

SIGSEGV at E91821

StateSolver > inspect pointer rax

RAX = 0x55550020

StateSolver > origin

Pointer originates from Allocation #31 / epoch 7.

StateSolver > lifetime

Allocation #31
created E18291
freed   E41882

Address reused by Allocation #64 at E44110.

StateSolver > why

Tracing dependencies...

E18291 malloc
   ↓
E18312 pointer returned
   ↓
E19288 pointer stored in Session.cached_user
   ↓
E41882 allocation freed
   ↓
E44110 address reused
   ↓
E91122 stale pointer dereferenced
   ↓
E91821 crash
```

---

# Harness API Boundary

Recommended architecture:

```text
                 StateSolver Core
                       |
                       v
                Analysis Service
                       |
                 Tool/API Layer
                       |
        +--------------+--------------+
        |              |              |
        v              v              v
       CLI            UI            Agent
                                      |
                                      v
                              Natural Language
```

This layer ensures the debugger logic remains independent of whichever interface is used.

---

# Stage 4 Suggested Modules

```text
harness/

├── api/
│   ├── state/
│   ├── history/
│   ├── allocations/
│   ├── replay/
│   └── causal/
│
├── session/
│   ├── context/
│   ├── selections/
│   └── investigation_state/
│
├── protocol/
│   ├── requests/
│   ├── responses/
│   └── transport/
│
├── agent/
│   ├── tools/
│   ├── planner/
│   ├── evidence/
│   └── verification/
│
└── cli/
```

---

# Final Four-Stage Architecture

```text
                          StateSolver

                     TARGET PROGRAM
                           |
                           v

              +--------------------------+
              | STAGE 1                  |
              | Trace Collection         |
              |                          |
              | registers                |
              | memory mappings          |
              | syscalls                 |
              | allocations              |
              | checkpoints              |
              +------------+-------------+
                           |
                           v
                       .ss TRACE
                           |
                           v

              +--------------------------+
              | STAGE 2                  |
              | Replay + Analysis        |
              |                          |
              | historical state         |
              | allocation epochs        |
              | causal reconstruction    |
              | crash explanation        |
              +------------+-------------+
                           |
                           v
                 STATE / CAUSAL MODEL
                           |
              +------------+-------------+
              |                          |
              v                          v

     +--------------------+       +----------------------+
     | STAGE 3            |       | STAGE 4              |
     | Visual UI          |       | Interaction Harness  |
     |                    |       |                      |
     | timeline           |       | CLI                  |
     | registers          |       | API                  |
     | memory             |       | agent tools          |
     | heap lifetimes     |       | NL interaction       |
     | causal graph       |       | IDE integration      |
     +--------------------+       +----------------------+
```

---

# Final Product Experience

The completed project should support three ways to investigate the same execution.

### Direct CLI

```bash
statesolver why rax --event 91800
```

### Visual exploration

```text
open trace
drag timeline
click allocation
inspect causal graph
```

### Interactive harness

```text
> Why did this crash?

> Where did that pointer originate?

> Was it freed?

> Show me when its address was reused.

> Jump to the event where the bug actually began.
```

All three interfaces should receive their answers from the **same StateSolver analysis engine**.

The long-term goal is therefore:

> **record a program once, then investigate its complete execution through commands, visualization, or an evidence-grounded debugging agent.**

