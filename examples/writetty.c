#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

#define STDIN 0
#define STDOUT 1

int main() {
    close(STDIN);
    // open("/dev/pts/4", O_WRONLY);
    openat(0, "/dev/pts/5", O_RDONLY, 0);
    getc(stdin);
    puts("Hello, world!");
    return 0;
}
