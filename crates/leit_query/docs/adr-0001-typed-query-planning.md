# ADR 0001: Typed query planning boundary

Status: Accepted for the typed-query landing slice.

## Decision

`Planner::plan_program` accepts the builder AST and shares term lowering with
textual planning. `ExecutionWorkspace` delegates to this planner and wraps every
external filter slot just as textual planning does. Terms bypass query syntax
parsing and reach the context dictionary unchanged, including punctuation and
operator-like text. Analysis is defined by the dictionary implementation:
`InMemoryIndex` applies its configured field analyzer and resolves a term only
when analysis produces exactly one token. Callers do not need to pre-normalize
terms for this index. Zero-token or multi-token analysis does not resolve a term.

Boost nodes multiply descendant term boosts. Factors, nested products, and the
final product with `PlanningContext::default_boost` must be finite and
non-negative. Invalid effective products return `QueryError::InvalidBoost`
identifying the term or phrase; invalid explicit factors identify their boost
node. Zero is allowed. This validation concerns multipliers, not a guarantee
against overflow in arbitrary scorer arithmetic.

Phrase nodes currently mean AND of their terms. Order and slop are ignored, and
different default fields may satisfy different terms. Empty phrases match
nothing. This is an explicit Phase 1 approximation, not positional matching.
Future positional support must use per-field phrases combined with OR and must
document its narrower result semantics.

## Limits and follow-up

The typed traversal rejects reachable cycles and computes depth iteratively with
memoization. Lowering can duplicate shared children and is limited by the emitted
node budget. Planning still allocates; reusable execution does not promise
allocation-free planning. The depth metadata follows logical lowering depth and,
like the existing textual planner, excludes extra term-expansion levels. A
follow-up should reconcile that with `QueryProgram::max_depth` documentation.

## Migration

Existing textual entry points remain available. Consumers can opt into
`QueryBuilder`, `UserQueryProgram`, and workspace `plan_program`/`search_program`.
Exhaustive matches on `QueryError` must handle the new `InvalidBoost` variant.
Typed callers that previously passed invalid context-derived boosts now receive
an error instead of a plan containing invalid score multipliers. No signature,
production dependency, or unsafe-code change is needed for this review fix.

## Validation

Unit regressions cover invalid context products, zero and finite products,
boolean and boost cycles, literal punctuation/operator terms, field analysis
(case and canonical normalization, zero/multiple tokens), multiple filter
slots, both scorers, and workspace reuse after errors and filtered searches.
Existing integration tests cover typed/text plan and hit parity, depth and node
budgets, phrases, nested boosts, and shared DAGs.
