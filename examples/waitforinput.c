#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main() {
    pid_t pid = getpid();
    printf("My PID: %d\n", pid);

    puts("Please enter your name:");
    char buffer[256];
    char* r = fgets(buffer, sizeof buffer / sizeof *buffer, stdin);
    if (r == NULL) {
        fputs("error while reading", stderr);
        return 1;
    }

    // remove trailing newline
    size_t n = strlen(buffer);
    buffer[n - 1] = '\0';

    printf("\nHello, %s!\n", buffer);

    return 0;
}
