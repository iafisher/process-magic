#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>

int main() {
    int prot = PROT_READ | PROT_WRITE;
    int flags = MAP_PRIVATE | MAP_ANONYMOUS;
    printf("prot=%d, flags=%d\n", prot, flags);
    void* r = mmap(0, 13, prot, flags, -1, 0);
    if (r == MAP_FAILED) {
        printf("error: %s\n", strerror(errno));
    } else {
        printf("r: %p\n", r);
    }
}
