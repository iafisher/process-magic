#include <fcntl.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <unistd.h>

#define STDIN 0
#define STDOUT 1
#define BUFSIZE 100

int main(int argc, char* argv[]) {
    // needs to be run as root
    char fpath[BUFSIZE];
    snprintf(fpath, BUFSIZE, "/proc/%s/fd/0", argv[1]);

    int fd = openat(0, fpath, O_WRONLY, 0);
    if (fd == -1) {
        perror("open file");
        return 1;
    }
    char *s = "Ian\n";

    while (*s) {
        int r = ioctl(fd, TIOCSTI, s);
        if (r == -1) {
            perror("ioctl");
            break;
        }
        s++;
    }

    puts("finished writing string to terminal");

    return 0;
}
