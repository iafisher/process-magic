#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main() {
    pid_t pid = getpid();
    printf("My PID: %d\n\n", pid);

    char* greeting = "Hello, world!";
    size_t n = strlen(greeting);
    char* buffer1 = malloc(n + 1);
    strcpy(buffer1, greeting);

    getc(stdin);

    char* buffer2 = malloc(n + 1);
    strcpy(buffer2, greeting);

    printf("buffer1: %s\n", buffer1);
    printf("buffer2: %s\n", buffer2);
    return 0;
}
