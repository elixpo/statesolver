/* Allocation-heavy microbenchmark: N malloc/free pairs with a live window,
 * plus light compute. Used to measure recording overhead per allocation. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 100000;
    void *live[64] = {0};
    unsigned long sum = 0;
    for (long i = 0; i < n; i++) {
        int slot = (int)(i % 64);
        free(live[slot]);
        live[slot] = malloc(16 + (size_t)(i % 200));
        memset(live[slot], (int)i, 16);
        sum += ((unsigned char *)live[slot])[3];
    }
    for (int i = 0; i < 64; i++) free(live[i]);
    printf("%lu\n", sum);
    return 0;
}
