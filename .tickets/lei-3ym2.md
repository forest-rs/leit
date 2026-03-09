---
id: lei-3ym2
status: open
deps: []
links: []
created: 2026-03-10T04:08:24Z
type: task
priority: 2
assignee: Norman Nunley, Jr
---
# leit_core: Memory and allocator abstractions

Add missing memory management abstractions to leit_core:
1. Arena trait for bump/arena allocation
2. BufferPool trait for block cursor support
3. Enhanced ScratchSpace with capacity management
4. String interning abstraction

These abstractions will enable custom allocators for embedded/no_std systems and improve query performance through arena allocation.

