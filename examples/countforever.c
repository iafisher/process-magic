#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define MILLIS_IN_NANOS (1000*1000)

void hide_cursor() { printf("\e[?25l"); }
void show_cursor() { printf("\e[?25h"); }

int main(int argc, char* argv[]) {
    pid_t pid = getpid();
    printf("My PID: %d\n\n", pid);

    hide_cursor();
    atexit(show_cursor);

    bool should_sleep = argc < 2 || strcmp(argv[1], "--no-sleep") != 0;

    int count = 0;
    while (1) {
        printf("\r%d", count);
        fflush(stdout);
        count++;

        if (should_sleep) {
            struct timespec ts;
            ts.tv_sec = 0;
            ts.tv_nsec = 300 * MILLIS_IN_NANOS;
            nanosleep(&ts, NULL);
        }
    }

    return 0;
}
