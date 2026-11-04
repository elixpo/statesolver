/* Use-after-free with address reuse (research direction §1).
 * A session caches a pointer to the user object. logout() frees the user but
 * forgets the cache. The next allocation of the same size reuses the address
 * for a Handler that holds a function pointer. A write through the stale
 * cache overwrites that function pointer, and the program crashes later
 * when the handler runs. Address-only tracing sees one address written
 * twice and never sees an object boundary. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ss_mark.h"

typedef struct { char name[16]; long score; void *pad; } User;      /* 32 bytes */
typedef struct { void (*run)(void); long id; char tag[16]; } Handler; /* 32 bytes */
typedef struct { User *cached_user; } Session;

static void greet(void) { puts("handler ok"); }

static User *load_user(const char *name) {
    User *u = malloc(sizeof *u);
    memset(u, 0, sizeof *u);
    strncpy(u->name, name, sizeof u->name - 1);
    return u;
}

static void logout(Session *s, User *u) {
    ss_mark("root-cause:free-without-clearing-cache");
    free(u); /* BUG: s->cached_user still points at u */
    (void)s;
}

int main(void) {
    Session s;
    User *u = load_user("alice");
    s.cached_user = u;
    ss_mark("stale-pointer-saved");
    logout(&s, u);

    Handler *h = malloc(sizeof *h); /* reuses u's address (tcache) */
    h->run = greet;
    h->id = 7;
    printf("user@%p handler@%p reused=%d\n", (void *)u, (void *)h, (void *)u == (void *)h);

    ss_mark("stale-write");
    s.cached_user->name[0] = 'A'; /* writes into h->run */
    memcpy(s.cached_user->name, "AAAAAAAA", 8);

    h->run(); /* SIGSEGV: corrupted function pointer */
    return 0;
}
