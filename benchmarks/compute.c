/* Compute-bound, allocation-free baseline: overhead here is syscall-only. */
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 100000000;
    unsigned long x = 1;
    for (long i = 0; i < n; i++) x = x * 6364136223846793005UL + 1442695040888963407UL;
    printf("%lu\n", x);
    return 0;
}
