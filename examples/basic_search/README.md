# basic_search

Minimal end-to-end example for the current Leit stack.

It shows how to:

- configure analyzers for indexed fields
- register field aliases used by the query planner
- add a few documents to an in-memory index
- execute a query with an explicit scorer
- rely on the default Unicode normalization path at both index and query time
- print ranked hits

Run it from the workspace root:

```bash
cargo run -p basic_search
```
