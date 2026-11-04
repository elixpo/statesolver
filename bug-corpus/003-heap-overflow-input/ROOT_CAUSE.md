# 003: heap overflow driven by external input

- **Symptom:** SIGSEGV on `act->cb()`, with `rip` derived from payload bytes (`0x4242424242424242`).
- **Root cause:** the length byte read from stdin is used unchecked (marker `root-cause:untrusted-length`).
- **Causal chain:** `read(0, hdr, 1)` (external input) → `len = hdr[0] = 48` → `memcpy(buf, payload, 48)` writes 24 bytes past `buf` (allocation X) into the adjacent allocation Y (`Action`), overwriting `cb` → indirect call faults.
- **Expected StateSolver answer:** the faulting `cb` value comes from payload bytes from the `read` syscall at input offset 1+32..1+40, written by the `memcpy` whose length comes from input byte 0. The write crosses from allocation X into Y. This tests "syscall input as a causal source" and object-boundary crossing.
