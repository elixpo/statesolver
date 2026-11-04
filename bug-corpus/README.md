# Bug corpus

Controlled and real C/C++ programs used to evaluate StateSolver (research direction §9; Stage 4, #4).

Each bug lives in its own directory:

```text
bug-corpus/<id>-<short-name>/
  src/              program source
  build.sh          deterministic build (records the compiler and flags)
  input/            inputs that trigger the bug
  ROOT_CAUSE.md     ground truth: the root-cause line or event, the bug class, and why address-only tracing is misleading
  meta.json         {class, reuse: bool, real_world: bool, source_url?, cve?}
```

Classes: UAF, UAF with address reuse, stale-pointer write, double free, heap overflow, wrong pointer propagation, corrupted function pointer, and corruption caused by external input. **At least a third of the corpus should be real bugs**, not only synthetic ones (see DISCOVERIES.md).
