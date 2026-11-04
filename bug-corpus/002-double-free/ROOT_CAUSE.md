# 002: double free from shared ownership

- **Symptom:** glibc aborts with `free(): double free detected in tcache 2` (SIGABRT) in the second `release`.
- **Root cause:** `make_pair` aliases `b->buf = a->buf` instead of copying (marker `root-cause:shared-ownership`).
- **Causal chain:** `malloc(48)` (allocation X) → pointer copied into `a.buf` and `b.buf` → `free(a.buf)` frees X → `free(b.buf)`: same pointer, X already dead.
- **Expected StateSolver answer:** the pointer passed to the second free originates from allocation X / epoch n via the copy in `make_pair`; X was freed at the first `release`.
