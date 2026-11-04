/* Ground-truth markers for the StateSolver bug corpus.
 *
 * ss_mark("label") is a no-op the recorder breakpoints by symbol name and
 * records as a `mark` event. Labels starting with "root-cause:" mark the
 * ground-truth root cause used for evaluation. Markers never change program
 * behaviour. */
#ifndef SS_MARK_H
#define SS_MARK_H
__attribute__((noinline, used, weak)) void ss_mark(const char *label) {
    __asm__ volatile("" : : "r"(label) : "memory");
}
#endif
