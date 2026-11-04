/* Double free caused by an ownership bug: a buffer is shared by two owners
 * and both release it. glibc detects the second free and aborts. The root
 * cause is the shared ownership set up in make_pair(), far from the crash. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ss_mark.h"

typedef struct { char *buf; } Owner;

static void make_pair(Owner *a, Owner *b) {
    a->buf = malloc(48);
    strcpy(a->buf, "shared");
    ss_mark("root-cause:shared-ownership");
    b->buf = a->buf; /* BUG: should be a copy */
}

static void release(Owner *o) { free(o->buf); }

int main(void) {
    Owner a, b;
    make_pair(&a, &b);
    char *noise[4];
    for (int i = 0; i < 4; i++) noise[i] = malloc(48); /* unrelated churn */
    release(&a);
    for (int i = 0; i < 4; i++) free(noise[i]);
    ss_mark("second-release");
    release(&b); /* SIGABRT: free(): double free detected */
    puts("unreachable");
    return 0;
}
