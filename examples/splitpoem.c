#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define CHUNK_SIZE 10
#define MILLIS_IN_NANOS (1000*1000)

int main(int argc, char* argv[]) {
    int slow = argc > 1 && strcmp(argv[1], "--slow") == 0;

    int counter = 1;
    while (1) {
        char buffer[CHUNK_SIZE];
        size_t nread = fread(buffer, sizeof *buffer, CHUNK_SIZE - 1, stdin);
        if (nread == 0) {
            break;
        }
        buffer[nread] = '\0';

        char fname[100];
        snprintf(fname, sizeof fname / sizeof *fname, "poem%0*d.txt", 3, counter);
        counter++;
        FILE* f = fopen(fname, "w");
        fwrite(buffer, sizeof *buffer, CHUNK_SIZE, f);
        fclose(f);

        if (slow) {
            printf("wrote %s\n", fname);

            struct timespec ts;
            ts.tv_sec = 0;
            ts.tv_nsec = 100 * MILLIS_IN_NANOS;
            nanosleep(&ts, NULL);
        }
    }

    return 0;
}
