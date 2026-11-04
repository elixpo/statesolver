# 001: use-after-free with address reuse

- **Symptom:** SIGSEGV when calling `h->run()`, with `rip = 0x4141414141414141`.
- **Root cause:** `logout()` frees the `User` but `Session.cached_user` keeps the pointer (marker `root-cause:free-without-clearing-cache`).
- **Causal chain:** `load_user` allocates User (epoch n) → the pointer is saved in `s.cached_user` → `free(User)` → `malloc(Handler)` reuses the address (epoch n+1) → `memcpy` through `s.cached_user` writes into the Handler's `run` field → indirect call faults.
- **Why address-only tracing misleads:** the last writer of the faulting function-pointer slot is the `memcpy` at `stale-write`, which writes through a pointer that "points at a valid live allocation" by address. Only epoch identity shows that the pointer belongs to the *freed* User epoch, not the live Handler.
- **Expected StateSolver answer:** the pointer used at `stale-write` originates from Allocation User / epoch n, freed at `logout`; the address was reused by Handler / epoch n+1.
