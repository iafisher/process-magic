#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define MILLIS_IN_NANOS (1000*1000)

void pause() {
    struct timespec ts;
    ts.tv_sec = 0;
    ts.tv_nsec = 300 * MILLIS_IN_NANOS;
    nanosleep(&ts, NULL);
}

int main(int argc, char* argv[]) {
    puts("ALERT!");
    pause();
    puts("ALERT!");
    pause();
    puts("ALERT!");
    pause();
    puts("This process has been taken over by the RC cyber army!");
    pause();
    return 0;
}
