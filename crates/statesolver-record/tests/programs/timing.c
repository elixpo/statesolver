/* Every time source a program can read without an obvious syscall. */
#include <stdio.h>
#include <sys/time.h>
#include <time.h>
#include <x86intrin.h>

int main(void) {
    struct timespec ts;
    struct timeval tv;
    clock_gettime(CLOCK_MONOTONIC, &ts);     /* vDSO */
    gettimeofday(&tv, NULL);                 /* vDSO */
    time_t t = time(NULL);                   /* vDSO */
    unsigned long long a = __rdtsc();
    unsigned int aux;
    unsigned long long b = __rdtscp(&aux);
    printf("%ld %ld %ld %d\n", (long)ts.tv_sec, (long)tv.tv_sec, (long)t, b >= a);
    return 0;
}
