#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

#define STDOUT 1

int main() {
    close(STDOUT);
    // open("/dev/pts/4", O_WRONLY);
    openat(0, "/dev/pts/4", O_WRONLY, 0);
    puts("Hello, world!");
    return 0;
}
