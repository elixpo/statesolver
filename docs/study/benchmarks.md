# Benchmarks: SEC-bench and ARVO / T2L-ARVO on a laptop

Status: research notes, 2026-11-05. Supports the laptop-scale plan in
[DIRECTION.md](../DIRECTION.md) (Changes, 2026-11-05) and the pilot gate in §2.
Facts were checked against the live data on 2026-11-05. **[verified]** means we ran
it on this laptop. **[unverified]** means it is documented but we have not run it.

## 1. SEC-bench (primary)

Paper: arXiv [2506.11791](https://arxiv.org/abs/2506.11791). Code: [github.com/SEC-bench/SEC-bench](https://github.com/SEC-bench/SEC-bench) (MIT).

### Data location

- HuggingFace: [`SEC-bench/SEC-bench`](https://huggingface.co/datasets/SEC-bench/SEC-bench) (license tag `mit`, not gated, last modified 2025-11-24).
  - `data/eval-cve.jsonl`: **200** instances (split `cve`; ids look like `njs.cve-2022-32414`).
  - `data/eval-oss.jsonl`: **100** instances (split `oss`; ids look like `libxml2.ossfuzz-42484562`).
  - Split `eval` is the union (300).
  - Direct download: `https://huggingface.co/datasets/SEC-bench/SEC-bench/resolve/main/data/eval-cve.jsonl` (and the same path for `eval-oss.jsonl`).
- Other datasets in the same org: `SEC-bench/Seed` (MIT; the seed set before verification) and `SEC-bench/SEC-bench-Pro` (Apache-2.0, arXiv 2605.26548; not examined here).
- Third-party add-on: [`shawnnnliu/Hardened_SEC-Bench`](https://huggingface.co/datasets/shawnnnliu/Hardened_SEC-Bench), created 2026-10-28. It has a `verified` config with 20 CVE instances. For each one it gives a manually verified developer patch, patch variants, and **root-cause analyses**. This is useful as a cross-check for our own annotations. Its ids are gpac.cve-2023-5586, libredwg.cve-2020-21814, libredwg.cve-2020-21816, mruby.cve-2022-0240, mruby.cve-2022-0631, mruby.cve-2022-1201, mruby.cve-2022-1276, mruby.cve-2022-1427, mruby.cve-2022-1934, mruby.cve-2022-0717, njs.cve-2019-13617, njs.cve-2021-46462, njs.cve-2022-28049, njs.cve-2022-29779, njs.cve-2022-31306, njs.cve-2022-31307, njs.cve-2022-32414, njs.cve-2022-34029, njs.cve-2022-43284, and njs.cve-2023-27727. The dataset has **no license tag**.

### Schema (one JSON object per line)

The field list comes from the dataset card and was checked against the records:

| Field | Meaning |
|---|---|
| `instance_id` | `<project>.<cve-id \| ossfuzz-id>` |
| `repo` | e.g. `nginx/njs` |
| `project_name` | e.g. `njs` |
| `lang` | `c` / `c++` |
| `work_dir` | source dir in the image, e.g. `/src/njs` |
| `sanitizer` | `address` (295/300), `undefined` (3), `memory` (2) |
| `bug_description` | short text (the NVD description for CVEs, an LLM-style summary for OSS-Fuzz cases) |
| `base_commit` | vulnerable commit |
| `build_sh` | build script (copied to `$SRC/build.sh`, run by OSS-Fuzz `compile`) |
| `secb_sh` | the `secb` helper script with the `build` / `repro` / `patch` functions; `repro()` holds the **exact PoC command** |
| `dockerfile` | instance Dockerfile (`FROM hwiwonlee/secb.base:latest`, clone plus checkout) |
| `patch` | **gold (developer) patch**, a unified diff; present for all 300 rows. A few OSS rows hold a `git show` of a merge commit rather than a plain diff (e.g. `mruby.ossfuzz-42538578`, `php.ossfuzz-427814456`), so check those before using them as ground truth |
| `exit_code` | exit code of the PoC after the gold patch |
| `sanitizer_report` | full ASan report (with alloc and free stacks for UAF) |
| `bug_report` | original issue text (GitHub issue, OSS-Fuzz) |

There is **no** separate CWE field. We derived the bug type from the first `ERROR: AddressSanitizer:` line of `sanitizer_report`. That gives, over all 300 rows: heap-buffer-overflow 114, SEGV 86, heap-use-after-free 42, stack-buffer-overflow 14, leaks 13, global-buffer-overflow 7, attempting double-free 2, bad free ("attempting free on address which was not malloc()-ed") 3, and a few others. Ground-truth **root-cause lines are not given**. Only the fix patch is, so our annotation layer is still needed.

Projects (counts): gpac 43, mruby 40, imagemagick 31, libxml2 31, njs 20, libredwg 20, php-src 19, faad2 12, matio 12, upx 11, exiv2 10, openexr 9, mupdf 8, openjpeg 5, libarchive 3, md4c 3, yara 3, wasm3 3, libdwarf-code 2, libiec61850 2, libheif 2, and one each of wabt, libsndfile, liblouis, jq, qpdf, readstat, libjpeg-turbo, libmodbus, libplist, libxls, and yaml-cpp.

### Docker images

- Registry: Docker Hub, `hwiwonlee/secb.eval.x86_64.<instance_id>`. All are amd64.
- Tags: `patch` and `poc` on all 300 instances, plus `latest` on the 200 CVE instances. The SEC-bench evaluator (`secb/evaluator/eval_instances.py`) uses `:patch` for the patching task and `:poc` for the PoC-generation task. **Use `:patch`.** It contains the PoC under `/testcase/` **[verified for jq]**. The `:poc` tag is for agents that must write the PoC; that it lacks the PoC is **[unverified]**.
- **Compressed size** (Docker Hub `full_size`, `:patch` tag): **0.90–1.76 GB**, with a median of about 0.98 GB. php-src, mupdf, matio-OSS, and mruby-OSS images are 1.16–1.76 GB.
- **Uncompressed on disk**: the jq image is **3.86 GB** **[verified]**. Budget about 4–6 GB per image plus the build tree. A pull took about 85 s here.
- Base: OSS-Fuzz `base-builder` on **Ubuntu 20.04** (glibc 2.31), with clang and `compile` **[verified]**.
- **The images are not prebuilt**: `/src/<proj>` holds source only, so you must run `secb build` first **[verified for jq; assumed for all, since the evaluator's patch script always runs `secb build`]**.

### Reproducing one instance (verified on this laptop with `jq.cve-2023-50246`)

```bash
ID=jq.cve-2023-50246
IMG=hwiwonlee/secb.eval.x86_64.$ID:patch
docker pull $IMG                                   # ~1 GB compressed, 3.9 GB on disk

# rr needs ptrace and perf_event_open in the container; pin to one P-core (CPUs 0-11 are P-core threads here)
docker run --rm -it --memory=5g --cpuset-cpus=2 \
  --cap-add=SYS_PTRACE --security-opt seccomp=unconfined \
  $IMG bash

# inside the container (WORKDIR is $work_dir, e.g. /src/jq):
secb build          # OSS-Fuzz `compile` with $SANITIZER (default: address). jq: ~41 s
secb repro          # runs the PoC command from secb_sh repro(), e.g. /src/jq/jq . /testcase/poc
                    # -> "ERROR: AddressSanitizer: heap-buffer-overflow ... WRITE of size 1 ... decToString"
                    # NB: `secb repro` exits 0 even on a crash; check stderr for "ERROR: AddressSanitizer"

# rr: the host rr binary does NOT run in the image (needs glibc 2.38 and libcapnp). Install the
# upstream release .deb, which works on Ubuntu 20.04:
curl -sSLO https://github.com/rr-debugger/rr/releases/download/5.9.0/rr-5.9.0-Linux-x86_64.deb
dpkg -i rr-5.9.0-Linux-x86_64.deb
rr record -o /tmp/trace /src/jq/jq . /testcase/poc     # exits 1 with the ASan report
rr replay -a /tmp/trace                                 # the same ASan report is replayed
rr pack /tmp/trace                                      # 26 MB for jq
# copy the trace out (docker cp, or a -v bind mount), then on the host:
docker rmi $IMG
```

All of the above ran successfully on this laptop (host `perf_event_paranoid=1`, i5-13450HX, rr 5.9.0 in the container). For the pipeline, bake rr into a thin derived image (`FROM $IMG` plus the `.deb`), or bind-mount a pre-extracted copy of the release package, so that rr is not downloaded once per bug.

The gold-patch check is the same as the SEC-bench evaluator's (`eval_patch_script.j2`): `cp patch.diff /testcase/model_patch.diff; secb patch; secb build; timeout 10 secb repro`, and the bug counts as fixed when no sanitizer error appears.

## 2. ARVO and T2L-ARVO

### ARVO

- Paper: arXiv [2408.02153](https://arxiv.org/abs/2408.02153), published at EuroS&P 2026. Meta repo: [github.com/n132/ARVO-Meta](https://github.com/n132/ARVO-Meta) (**BSD-2-Clause**). Generator: [github.com/n132/ARVO](https://github.com/n132/ARVO).
- **Current data**: release asset `arvo.db` (SQLite, 155 MB, [v3.0.0](https://github.com/n132/ARVO-Meta/releases/download/v3.0.0/arvo.db)). One table `arvo` with **6,138 rows**. Its ids are the **new OSS-Fuzz issue-tracker ids** (40096184–438309779). Schema:
  `localId, project, reproduced, reproducer_vul, reproducer_fix, patch_located, patch_url, verified, fuzz_target, fuzz_engine, sanitizer, crash_type, crash_output, severity, report, fix_commit, language, repo_addr, submodule_bug`.
  In v3.0.0, `reproduced` and `patch_located` are true for all 6,138 rows and `verified` is 0 for all of them. ARVO says its located fix commits are about 80% correct.
- **Legacy data** (old Monorail ids, matching the first ARVO paper and **T2L-ARVO**): `archive_data/meta/<id>.json` (fields `fix`, `verify`, `localId`, `project`, `fuzzer`, `sanitizer`, `crash_type`, `severity`, `report`) and `archive_data/patches/<id>.diff` (the located fix diff), e.g. [`archive_data/meta/14245.json`](https://github.com/n132/ARVO-Meta/blob/main/archive_data/meta/14245.json). The README says this folder is frozen.
- **Images**: Docker Hub `n132/arvo:<localId>-vul` and `n132/arvo:<localId>-fix` (legacy ids, e.g. `n132/arvo:14245-vul`). Compressed size for the T2L cases: **0.71–2.5 GB, mostly 1.0–1.5 GB**. llvm-project 20785 is **7.6 GB**, and gdal, graphicsmagick, and binutils are 2.0–2.5 GB.
- **Reproduce** (from the README):
  ```bash
  docker run -it n132/arvo:25402-vul arvo            # feeds the PoC to the vulnerable binary, prints the ASan report
  docker run -it n132/arvo:25402-vul bash            # interactive; inside: `arvo compile` to rebuild, then `arvo`
  docker run -it n132/arvo:25402-fix bash            # fixed version
  ```
  README known issue: some cases fail to recompile on recent kernels (MSan parsing `/proc/*/maps`). Their suggested workaround is to disable ASLR. The ARVO image layout (prebuilt binary or not) is in §4.

### T2L-ARVO (the 50 cases)

- Paper: arXiv [2510.02389](https://arxiv.org/abs/2510.02389), "From Trace to Line". It claims the code and benchmark are at `github.com/haoranxi/T2LAgent`, but **that repository returns 404 as of 2026-11-05**. The paper's reproducibility statement says the code will be released on publication. **The line-level ground truth is therefore not publicly available.** The paper says the ground truth is "exact line matches to ground-truth patches", i.e. derived from the ARVO fix diff, with no per-line file. Ask the authors (Haoran Xi, NYU) or derive the lines ourselves from `archive_data/patches/<id>.diff`.
- The **case ids** are listed in the paper's Appendix A.4, Table 8. It has 49 rows, because the "System & Runtime Errors" family lists only 9. All 49 resolve in ARVO-Meta `archive_data/meta` and `archive_data/patches`, and all have a `-vul` image on Docker Hub **[verified 2026-11-05]**.

| Family | ARVO ids (project, crash type) |
|---|---|
| Buffer overflow | 16614, 16615 opensc HBO; 13956 yara HBO; 20856 ndpi HBO; 17330, 17297 openthread SBO; 42454 ghostpdl SBO; 18562 lwan GBO; 30507 serenity GBO; 18231 binutils-gdb GBO |
| Uninit / unknown state | 49493, 57037 mruby SEGV; 24290 libvips SEGV; 23778 binutils-gdb UUV; 20112 open62541 UUV; 47855 harfbuzz UUV; 16857 matio UUV; 43989, 45320 ghostpdl null-deref; 2623 h2o null-deref |
| **Memory lifecycle** | **42503 php-src UAF; 38878 harfbuzz UAF; 14245 karchive UAF; 19723 leptonica UAF; 33750, 34116 fluent-bit double-free**; 20785 llvm-project, 3505 librawspeed, 51687 mongoose use-after-poison; 31705 c-blosc2 invalid-free |
| Type / param | 2798 gdal, 29267 serenity bad-cast; 33150 libredwg, 20217 arrow object-size; 12679 openthread, 23547 php-src memcpy-overlap; 25357 libsndfile, 60605 ndpi negative-size; 2692 boringssl bad fn-ptr; 50623 serenity VLA bound |
| System / runtime | 16737, 7966, 7654, 7639 graphicsmagick unknown signal; 59193 faad2, 49915 ndpi, 48780 libvpx check-failed; 7361 ots, 32939 rdkit bad sanitizer annotation |

For our study, the useful T2L cases are the memory-lifecycle ones plus a few HBOs (see the candidate table). The other families (uninit/MSan, UBSan, check-failed) do not exercise allocation lifetimes.

## 3. Licensing and terms

- **SEC-bench**: dataset and code are **MIT** (HF tag plus repo LICENSE). Images bundle upstream project source under each project's own license (GPL for exiv2, libredwg, and others; MIT or BSD for many). That does not matter for research use. Do not redistribute modified images without keeping those licenses.
- **SEC-bench-Pro**: Apache-2.0 (not used).
- **Hardened_SEC-Bench**: no license stated. Cite it, and do not redistribute it until the license is clarified.
- **ARVO-Meta**: **BSD-2-Clause**. The underlying reports are from the OSS-Fuzz tracker (public after disclosure). The PoCs are fuzzer outputs.
- **T2L-ARVO**: no license stated, and the artifacts are not released. Citing the paper's table of ids is fine. If we derive labels from ARVO patches, they are ours to release (as BSD-2 derived data with attribution).
- None of these datasets has a click-through or gated access, and none has a stated non-commercial clause. Our released labels should cite SEC-bench (MIT) and ARVO (BSD-2) and keep their notices.

## 4. ASan, and producing a non-ASan build of the same commit

This matters because ASan replaces the allocator. It intercepts `malloc`/`free`, uses its own size-class allocator with redzones, and puts freed chunks in a **quarantine** (256 MB by default), so a freed chunk is **not reallocated** during a typical PoC run. Under ASan, the "freed chunk gets reallocated" pattern our allocation epochs target almost never happens. ASan also reports and aborts at the first bad access, before any downstream corruption.

**SEC-bench**: every image builds with OSS-Fuzz `compile`, using `SANITIZER=address` set in the image env (295/300 instances; 3 UBSan, 2 MSan). `compile` appends `$SANITIZER_FLAGS_${SANITIZER}` (`-fsanitize=address -fsanitize-address-use-after-scope`) to `CFLAGS`/`CXXFLAGS`. A **non-ASan build of the same commit takes one environment variable**:

```bash
docker run ... -e SANITIZER=none $IMG bash -c 'secb build && /src/jq/jq . /testcase/poc'
```

**[verified for jq]**: the binary has 0 `__asan_` symbols, and the PoC then dies in glibc with `free(): invalid pointer` (SIGABRT, exit 134), i.e. the overflow corrupts heap metadata and is caught later, at a different site. `SANITIZER=none` is a supported value in base-builder's `compile` (it skips the sanitizer flags). Caveats:
- Some `build_sh` scripts link a libFuzzer harness or add their own flags. For the CLI-tool repro commands in our candidate list (xmllint, mruby, njs, jq, MP4Box, and similar), `none` should work, but **check each one** **[unverified beyond jq]**.
- Without ASan, many PoCs **do not crash at all** (a silent UAF read or a small overflow). The ground-truth "crash" for the agent must then come from the ASan run, or from glibc checks (`MALLOC_CHECK_=3`, `GLIBC_TUNABLES=glibc.malloc.tcache_count=0`, and so on).
- Rebuild in a fresh container (or `make clean`). An in-place rebuild may reuse ASan objects.

**Middle option, with no rebuild**: keep the ASan build and run with `ASAN_OPTIONS=quarantine_size_mb=0:thread_local_quarantine_size_kb=0` (optionally `:detect_leaks=0`). Freed chunks then become reusable immediately, so a UAF read can hit a *reallocated* chunk. ASan then reports it only if the chunk is still free; if it was reallocated, the access looks valid. This gives "reuse-present" behaviour while keeping ASan's alloc and free stacks for ground truth. Recommendation: record **two traces per bug**, (a) the default ASan run for the crash and ground truth, and (b) the `SANITIZER=none` build, or `quarantine_size_mb=0`, for the reuse stratum, and report which allocator each trace used.

**Custom allocators** also hide lifetimes from a malloc-level epoch tracker, independent of ASan:
- php-src uses the Zend MM unless `USE_ZEND_ALLOC=0`, which SEC-bench's repro commands already set.
- mruby routes everything through `mrb_default_allocf`, which calls `realloc`.
- libxml2 uses `xmlMalloc`/`xmlRealloc` wrappers. These end in libc, so that is fine.
- gpac uses `gf_malloc`/`gf_free`, which are thin wrappers.
- yara 3.5 frees through its arena (`yr_arena_destroy`).

**ARVO**: images are also OSS-Fuzz builds. `sanitizer` is per record (asan for all memory-lifecycle T2L cases; MSan or UBSan for some others). They are rebuilt with `arvo compile`, which uses the image's OSS-Fuzz env, so in principle `SANITIZER=none arvo compile` gives the same switch. ARVO binaries are libFuzzer/AFL **fuzz targets** (e.g. `kimgio_fuzzer`), not CLI tools. A non-ASan libFuzzer target still links the libFuzzer driver, but it can run a single input. **[unverified; see §4a]**

### 4a. ARVO image check (verified with `n132/arvo:13956-vul`, yara)

- The image is **3.21 GB on disk** (0.85 GB compressed). The pull took about 3 min.
- Base is **Ubuntu 16.04** (glibc 2.23). Old cases use old OSS-Fuzz bases.
- The fuzz targets are **prebuilt** in `/out/` (`elf_fuzzer`, `pe_fuzzer`, …). The PoC is `/tmp/poc`. `CFLAGS=-O1 -fno-omit-frame-pointer -gline-tables-only`.
- `/bin/arvo` is a small script. It exports `SANITIZER=address`, `FUZZING_ENGINE=libfuzzer`, and an ASan option set that includes **`quarantine_size_mb=10`** (smaller than the default 256 MB, so reuse is more likely than in SEC-bench). It then runs `/out/<fuzzer> /tmp/poc`, or `compile` when called as `arvo compile`.
- **Because the script hard-codes `SANITIZER=address`, `-e SANITIZER=none` has no effect on `arvo compile`.** For a non-ASan build, call base-builder's `compile` directly with `SANITIZER=none` exported **[unverified]**. Note that the libFuzzer engine is still linked unless `FUZZING_ENGINE=none` is also set (then OSS-Fuzz links the standalone driver) **[unverified]**.
- `docker run --rm n132/arvo:13956-vul arvo` reproduces the ASan heap-buffer-overflow **[verified]**.
- The image has **no curl or wget**. **The rr 5.9.0 release `.deb` does not run on Ubuntu 16.04** (it needs GLIBC_2.29) **[verified]**. Alternative: copy the prebuilt target and PoC out (`docker create` plus `docker cp /out/<fuzzer> /tmp/poc`) and record with the **host** rr. Old-glibc binaries run on the newer host glibc. Results:
  - **Host rr directly on the host fails.** rr 5.9.0 on host kernel 7.0 with host glibc hits `FATAL record_syscall.cc:6733 … Expected EINVAL for 'madvise' but got result 0; unknown madvise(102)`. Advice 102 is `MADV_GUARD_INSTALL` (Linux 6.13+), which the new host glibc probably uses for thread stacks. Avoid recording on the host until rr supports it.
  - **This works [verified]**: copy `/out/elf_fuzzer` and `/tmp/poc` out of the ARVO image, then record inside a stock `ubuntu:22.04` container with the rr release `.deb` installed and the files bind-mounted:
    ```bash
    C=$(docker create n132/arvo:13956-vul); docker cp $C:/out/elf_fuzzer w/; docker cp $C:/tmp/poc w/; docker rm $C
    docker run --rm --cpuset-cpus=2 --cap-add=SYS_PTRACE --security-opt seccomp=unconfined \
      -v $PWD/rrdeb:/rrdeb:ro -v $PWD/w:/w ubuntu:22.04 bash -c \
      'dpkg -i /rrdeb/rr-5.9.0-Linux-x86_64.deb && rr record -o /w/tr /w/elf_fuzzer /w/poc; rr replay -a /w/tr'
    ```
    The recording reproduced the ASan heap-buffer-overflow, and so did the replay. The trace was 14 MB. The source tree for the agent stays in the ARVO image (`/src`), so copy it out too.
  - The same `madvise(102)` risk applies to any SEC-bench binary recorded on the host. Inside the SEC-bench image (Ubuntu 20.04 glibc), recording works, as shown in §1.

## 5. Feasibility filter used

Keep an instance when all of the following hold:
- `sanitizer=address`;
- the ASan class is heap-use-after-free, double-free, bad-free, or heap-buffer-overflow;
- the repro is a single CLI process (no server, no threads in the PoC path);
- the project is small or medium: libxml2, mruby, njs, yara, jq, libarchive, openjpeg, libdwarf, md4c, faad2, libplist, exiv2, qpdf, libheif, and gpac, which is medium (~1 GB image, builds MP4Box);
- the image is at most about 1.2 GB compressed.

php-src and imagemagick are borderline because their builds are heavy for 6 GB RAM (use `-j2`). Excluded: mupdf (1.7 GB), anything that needs a fuzz-target repro, and leak-only, UBSan, or MSan cases.

"Distance" in the tables compares the top non-allocator frame of the **use** stack with that of the **free** stack in the ASan report (same function / same file / cross-file). It is a crude proxy for crash-to-root-cause distance. The real root cause still needs annotation.

**Reuse** cannot be read from an ASan report, because quarantine prevents it. Whether a chunk is reallocated must be measured on the non-ASan or `quarantine_size_mb=0` trace (§4). Bugs whose free comes through `realloc` (marked "realloc") are the most likely to show address reuse.

## 6. Candidate table (40 instances; every id seen in the data on 2026-11-05)

Size is the Docker Hub compressed size of `:patch` (SEC-bench) or `-vul` (ARVO). GT patch: SEC-bench `patch` field (developer fix) / ARVO `archive_data/patches/<id>.diff` (located fix, about 80% accurate). **P** marks the recommended pilot set.

### SEC-bench (33)

| # | instance_id | Project | Bug type (ASan) | Use → free sites; distance | Size | GT patch | P |
|---|---|---|---|---|---|---|---|
| 1 | libxml2.ossfuzz-42484562 | libxml2 | heap-use-after-free READ 4 | xmlValidateRef (valid.c) → xmlTextReaderFreePropList (xmlreader.c); cross-file | 0.98 GB | yes | **P** |
| 2 | libxml2.ossfuzz-42486665 | libxml2 | heap-use-after-free READ 8 | xmlNodeDumpOutputInternal (xmlsave.c) → xmlFreeNs (tree.c); cross-file | 0.98 GB | yes | **P** |
| 3 | libxml2.ossfuzz-42537493 | libxml2 | heap-use-after-free READ 4 | xmlCharEncCloseFunc, freed in the same function; same-fn | 0.98 GB | yes (3 files) | **P** |
| 4 | libxml2.ossfuzz-42529030 | libxml2 | heap-use-after-free READ 1 | xmlParserPrintFileContextInternal (error.c) → xmlReallocLoc; cross-file, **realloc** | 0.98 GB | yes | **P** |
| 5 | libxml2.ossfuzz-42517254 | libxml2 | heap-use-after-free READ 4 | xmlXIncludeCopyXPointer → xmlXIncludeIncludeNode (xinclude.c); same-file | 0.98 GB | yes | |
| 6 | mruby.cve-2022-1934 | mruby | heap-use-after-free READ 8 | hash_new_from_values (vm.c) → mrb_default_allocf; cross-file (Hardened-verified) | 0.96 GB | yes | **P** |
| 7 | mruby.ossfuzz-42511322 | mruby | heap-use-after-free READ 8 | stack_copy (vm.c) → mrb_realloc_simple (gc.c); cross-file, **realloc** (VM stack realloc) | 1.16 GB | yes | **P** |
| 8 | mruby.cve-2022-1212 | mruby | heap-use-after-free READ 1 | str_escape (string.c) → mrb_default_allocf; cross-file | 0.96 GB | yes | **P** |
| 9 | mruby.cve-2018-10199 | mruby | heap-use-after-free READ 4 | fptr_finalize (io.c) → allocf; cross-file (old mruby 1.4, `build/host/bin/mruby`) | 0.97 GB | yes | |
| 10 | njs.cve-2022-27007 | njs | heap-use-after-free WRITE 88 | njs_function_frame_alloc → njs_function_frame_free; same-file | 0.97 GB | yes | **P** |
| 11 | yara.cve-2017-5924 | yara | heap-use-after-free READ 8 | yr_compiler_destroy (compiler.c) → yr_arena_destroy (arena.c); cross-file | 0.97 GB | yes | **P** |
| 12 | yara.cve-2016-10211 | yara | heap-use-after-free READ 1 | yr_parser_lookup_loop_variable → yydestruct (grammar.y); cross-file; the patch includes regenerated grammar (650 lines) | 0.97 GB | yes (large) | |
| 13 | qpdf.cve-2021-36978 | qpdf | heap-use-after-free WRITE 1 | Pl_ASCII85Decoder::write → ~runtime_error (libstdc++); cross-file; repro via `qpdf_fuzzer` | 1.01 GB | yes (5 files) | **P** |
| 14 | libheif.cve-2023-49464 | libheif | heap-use-after-free READ 2 | UncompressedImageCodec::get_luma_bits_per_pixel → std::vector dealloc; cross-file (C++) | 1.00 GB | yes | **P** |
| 15 | gpac.cve-2023-48011 | gpac | heap-use-after-free READ 8 | flush_ref_samples (movie_fragments.c) → gf_isom_box_del (box_funcs.c); cross-file | 0.98 GB | yes | **P** |
| 16 | gpac.cve-2022-1795 | gpac | heap-use-after-free READ 8 | gf_node_try_destroy → gf_node_unregister (base_scenegraph.c); same-file | 0.98 GB | yes | |
| 17 | gpac.cve-2023-4755 | gpac | heap-use-after-free READ 8 | mp4_mux_process_fragmented → mp4_mux_configure_pid (mux_isom.c); same-file | 0.98 GB | yes | |
| 18 | php.cve-2018-12882 | php-src | heap-use-after-free READ 8 | _php_stream_free, freed in the same function; `USE_ZEND_ALLOC=0` | 1.00 GB | yes | |
| 19 | php.ossfuzz-42515276 | php-src | heap-use-after-free READ 4 | php_date_initialize → update_errors_warnings (php_date.c); same-file; heavy build | 1.64 GB | yes | |
| 20 | imagemagick.cve-2019-17541 | imagemagick | heap-use-after-free READ 8 | DestroyStringInfo (string.c) → RelinquishMagickMemory; cross-file; heavy build | 0.99 GB | yes | |
| 21 | gpac.cve-2023-4679 | gpac | **attempting double-free** | gf_filterpacket_del (filter.c) ← first free in Media_GetSample (media.c); cross-file | 0.98 GB | yes | **P** |
| 22 | gpac.cve-2023-48013 | gpac | **attempting double-free** | gf_filterpacket_del ← Media_GetSample; cross-file. Patches the same file as #21 (isoffin_read_ch.c), so likely a sibling bug | 0.98 GB | yes | **P** |
| 23 | exiv2.cve-2017-14857 | exiv2 | bad free (not malloc()-ed) | `operator delete` on an interior pointer (image.cpp) | 1.01 GB | yes | **P** |
| 24 | libxml2.ossfuzz-42509369 | libxml2 | bad free (not malloc()-ed) | xmlFreeNodeList / xmlFreeNode on dict-owned memory; the "patch" is a revert commit | 0.98 GB | yes (revert) | |
| 25 | jq.cve-2023-50246 | jq | heap-buffer-overflow WRITE 1 | decToString (decNumber.c); **fully verified end-to-end with rr** | 0.96 GB | yes | **P** |
| 26 | libarchive.cve-2020-21674 | libarchive | heap-buffer-overflow WRITE 1 | archive_string_append_from_wcs (archive_string.c) | 0.99 GB | yes | **P** |
| 27 | libarchive.cve-2017-14503 | libarchive | heap-buffer-overflow READ 2 | lha_crc16 / lha_read_data_none | 0.98 GB | yes | |
| 28 | openjpeg.cve-2021-3575 | openjpeg | heap-buffer-overflow READ 4 | sycc420_to_rgb (color.c) | 0.96 GB | yes | **P** |
| 29 | openjpeg.cve-2024-56827 | openjpeg | heap-buffer-overflow WRITE 8 | opj_j2k_add_tlmarker (j2k.c) | 0.96 GB | yes | |
| 30 | libdwarf.cve-2022-34299 | libdwarf-code | heap-buffer-overflow READ 8 | dwarf_global_formref_b (dwarf_form.c) | 0.96 GB | yes | **P** |
| 31 | md4c.cve-2018-11545 | md4c | heap-buffer-overflow | md_merge_lines (md4c.c); single-file project | 0.96 GB | yes | **P** |
| 32 | faad2.cve-2021-32272 | faad2 | heap-buffer-overflow WRITE 4 | stszin (mp4read.c) | 0.96 GB | yes | |
| 33 | yara.cve-2023-40857 | yara | heap-buffer-overflow READ 8 | yr_execute_code (exec.c) | 0.96 GB | yes | |

Other feasible SEC-bench HBOs seen in the data, kept as reserves: libplist.cve-2017-5545, njs.cve-2019-13617 (Hardened-verified), libjpeg-turbo.cve-2020-13790, libxls.cve-2023-38855, exiv2.cve-2018-17229, matio.ossfuzz-42535678, wasm3.ossfuzz-42495805, and openexr.ossfuzz-42517450. Further UAF reserves: libxml2.ossfuzz-42486772, -42487244, -42487785, and -42531481, and mruby.ossfuzz-42538578 (its patch is a merge commit).

### T2L-ARVO (7; adds line-level-labelled cases once labels exist)

| # | ARVO id | Project | Crash type | Notes | Size (`-vul`) | GT patch |
|---|---|---|---|---|---|---|
| 34 | 38878 | harfbuzz | Heap-use-after-free READ 4 | medium C++ library; fuzz-target repro | 0.97 GB | ARVO diff |
| 35 | 19723 | leptonica | Heap-use-after-free READ {*} | medium C | 1.38 GB | ARVO diff |
| 36 | 14245 | karchive | Heap-use-after-free READ 8 | Qt dependency (heavier build); AFL target `kimgio_fuzzer` | 1.35 GB | ARVO diff |
| 37 | 34116 | fluent-bit | Heap-double-free | | 1.18 GB | ARVO diff |
| 38 | 33750 | fluent-bit | Heap-double-free | | 1.58 GB | ARVO diff |
| 39 | 31705 | c-blosc2 | Invalid-free | small C | 1.50 GB | ARVO diff |
| 40 | 13956 | yara | Heap-buffer-overflow READ 1 | small; overlaps a SEC-bench project | 0.85 GB | ARVO diff |

Skipped T2L memory-lifecycle cases: 42503 php-src UAF (1.49 GB, heavy build), 20785 llvm-project (7.6 GB), and use-after-poison cases (3505, 51687), which are custom-pool poisoning rather than malloc lifetimes.

## 7. Recommended pilot (20, all SEC-bench, matching DIRECTION §2 "about 20 SEC-bench bugs")

- **UAF (12)**: libxml2.ossfuzz-42484562, libxml2.ossfuzz-42486665, libxml2.ossfuzz-42537493, libxml2.ossfuzz-42529030, mruby.cve-2022-1934, mruby.ossfuzz-42511322, mruby.cve-2022-1212, njs.cve-2022-27007, yara.cve-2017-5924, qpdf.cve-2021-36978, libheif.cve-2023-49464, gpac.cve-2023-48011.
- **Double free / bad free (3)**: gpac.cve-2023-4679, gpac.cve-2023-48013, exiv2.cve-2017-14857. These are the only 2 true double frees in SEC-bench, plus one bad free.
- **Heap overflow (5)**: jq.cve-2023-50246, libarchive.cve-2020-21674, openjpeg.cve-2021-3575, libdwarf.cve-2022-34299, md4c.cve-2018-11545.

This covers 13 projects. Distance mix for the 15 lifetime bugs: 11 cross-file, 2 same-file, and 2 same-function or local. Two cases free through `realloc` (#4, #7), which makes them the most likely reuse candidates. The A5 (no-epoch) ablation needs the reuse stratum, so record the non-ASan or `quarantine_size_mb=0` trace for all 15 lifetime bugs and keep the ones where the freed chunk is actually reallocated before the bad access. If too few show reuse, swap in reserves from #5, #9, and #16–#20.

**Disk and time estimate**: about 1 GB download and about 4–6 GB on disk per image, deleted after `rr pack`. The traces are about 25–200 MB each **[only jq measured: 26 MB]**. Serial pull, build, record, and pack is roughly 3–10 min per bug, and longer for gpac, qpdf, and libheif.

## 8. Open items

- Run the §1 procedure on each pilot bug. Confirm that `SANITIZER=none` builds and the repro command work, and record build peak RSS under `--memory=5g`.
- ARVO: script the copy-out and record flow from §4a, and test a non-ASan `compile` (`SANITIZER=none FUZZING_ENGINE=none`) in one image.
- Report the rr `madvise(102)` (`MADV_GUARD_INSTALL`) failure on kernel 7.0 / new glibc upstream, or check rr master. Until then, never record on the host directly.
- Email the T2L authors for the per-line labels, or derive lines from the ARVO diffs under our annotation guideline.
- Confirm the Hardened_SEC-Bench license before reusing its root-cause analyses.
