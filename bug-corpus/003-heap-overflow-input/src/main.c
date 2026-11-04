/* Heap overflow driven by external input: a length field read from stdin
 * is trusted, and the copy overruns a 24-byte buffer into the adjacent
 * allocation's function pointer. Run with input/overflow.bin on stdin. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include "ss_mark.h"

typedef struct { void (*cb)(void); char label[24]; } Action;

static void ok(void) { puts("action ok"); }

int main(void) {
    unsigned char hdr[1];
    char payload[256];
    if (read(0, hdr, 1) != 1) return 1;
    ssize_t n = read(0, payload, sizeof payload);
    if (n <= 0) return 1;

    char *buf = malloc(24);
    Action *act = malloc(sizeof *act); /* allocated right after buf */
    act->cb = ok;

    ss_mark("root-cause:untrusted-length");
    size_t len = hdr[0]; /* BUG: no bound check against 24 */
    memcpy(buf, payload, len);

    act->cb(); /* SIGSEGV: cb overwritten by payload bytes */
    free(act);
    free(buf);
    return 0;
}
