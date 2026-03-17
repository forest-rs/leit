# Indexed RPN Posting Tape — Design Notes

Status: brainstorming (incomplete)

## Core Idea

Replace the current `BTreeMap<TermId, Vec<PostingEntry>>` with a flat
instruction tape where each posting is encoded as a small group of
fixed-size instructions in Indexed RPN form. The scorer is "compiled"
into the tape at index time rather than interpreted at query time.

Inspired by the Indexed RPN work in FMPL / Carbon's SemIR: a flat
instruction array where each instruction's result is stored at its
index, and operands reference other instructions by index.

## Design Decisions Made

- **Partial pre-computation**: TF saturation and length normalization
  baked into the tape at index time. IDF and boost applied at query
  time. Changing k1/b requires re-encoding the tape (not re-indexing
  raw data).

- **New `leit_tape` crate**: Opt-in alongside existing postings.
  Allows incremental adoption and A/B comparison.

- **Self-describing blocks**: Each block is a sub-tape with a header
  instruction encoding its upper bound, posting count, and skip
  offset. Block pruning = compare `upper_bound * idf * boost` against
  threshold, jump to skip offset.

- **Fixed-size 16-byte instructions**: 4-byte opcode + 3x 4-byte
  operands. Little-endian, no pointers, mmap-ready from day one.

- **Approach A (Pure Indexed RPN)**: Scoring compiled into the tape.
  Simplest path. Raw data can regenerate tapes if scorer params change.

## Instruction Format (Draft)

Every instruction is exactly 16 bytes:

```
bytes 0-3:   opcode (u32)
bytes 4-7:   operand_a (u32 or f32)
bytes 8-11:  operand_b (u32 or f32)
bytes 12-15: operand_c (u32 or f32)
```

### Opcodes (Draft)

| Opcode | Name        | a                  | b                  | c                |
|--------|-------------|--------------------|--------------------|------------------|
| 0      | TapeHeader  | term_id (u32)      | instr_count (u32)  | version (u32)    |
| 1      | BlockHeader | posting_count (u32)| skip_to (u32)      | upper_bound (f32)|
| 2      | DocId       | doc_id (u32)       | reserved           | reserved         |
| 3      | TfSat       | tf_saturation (f32)| reserved           | reserved         |
| 4      | Emit        | doc_ref (u32 idx)  | score_ref (u32 idx)| reserved         |
| 5      | End         | reserved           | reserved           | reserved         |

### Example: Block with 2 postings

```
[0] TapeHeader { term_id: 7, instr_count: 9, version: 1 }
[1] BlockHeader { posting_count: 2, skip_to: 8, upper_bound: 0.87 }
[2] DocId(42)
[3] TfSat(0.72)
[4] Emit { doc_ref: 2, score_ref: 3 }
[5] DocId(57)
[6] TfSat(0.65)
[7] Emit { doc_ref: 5, score_ref: 6 }
[8] End
```

### Execution Model

1. Load collection-level stats: `idf`, `boost` (query-time values)
2. Linear scan through tape instructions
3. At `BlockHeader`: check `upper_bound * idf * boost <= threshold` → skip to `skip_to`
4. At `Emit`: compute `tape[score_ref].tf_sat * idf * boost`, collect with `tape[doc_ref].doc_id`

Per-posting cost at query time: one multiply + one compare (vs current BM25 full computation).

## Open Questions

- **Variable-length data (positions, multi-field scores)**: The 16-byte
  fixed instruction format doesn't naturally support variable-length
  payloads. Options discussed:
  - Continuation opcodes (chain multiple instructions)
  - Payload bit in opcode (bit 31 = variable payload follows)
  - Two instruction sizes (16-byte narrow, 32-byte wide)
  - Not yet decided.

- **Opcode extensibility**: What other indexing methods need representation?
  Positional data for phrase queries, BM25F multi-field, numeric ranges,
  delta-encoded doc IDs, skip pointers. The fixed 3-operand format may
  need extension.

- **Tape construction API**: How does the builder create tapes? Needs
  access to BM25 params + field stats at index time to pre-compute
  TF saturation.

- **Relationship to segment format**: Does the tape replace the
  PostingsMetadata + PostingsPayload sections in the segment codec,
  or sit alongside them?

- **Cache/regeneration**: If k1/b change, can we regenerate tapes from
  the raw posting data without re-analyzing documents? (Yes, if we
  keep raw tf + doc_length somewhere.)

## Context

- Conversation date: 2026-03-15
- Originated from discussion of aligning leit with forest-rs conventions,
  which led to a tangent about whether a bytecode/VM approach makes sense.
- Key insight: not at the query tree level (already essentially Indexed
  RPN via the QueryProgram arena), but at the postings level where the
  tight scoring loop runs per-document.
- Related prior art: FMPL Indexed RPN IR, Carbon SemIR, execution_tape
  register-based VM.
