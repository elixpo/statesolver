#include <pthread.h>
#include <stdio.h>
static void *work(void *p) { return p; }
int main(void) {
    pthread_t t;
    pthread_create(&t, NULL, work, NULL);
    pthread_join(t, NULL);
    puts("done");
    return 0;
}
