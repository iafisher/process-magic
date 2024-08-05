#include <signal.h>
#include <stdio.h>
#include <unistd.h>

void do_child() {
    // setpgid(0, 0);
    raise(SIGSTOP);
}

void do_parent(pid_t child) {
    printf("Parent PID: %d\n", getpid());
    printf("Child PID:  %d\n", child);
    while (1) {}
}

int main(int argc, char* argv[]) {
    if (argc != 2) {
        fputs("error: expected one argument\n", stdout);
        return 1;
    }

    pid_t r = fork();
    switch (r) {
        case -1:
            perror("fork");
            return 1;
        case 0:
            do_child();
        default:
            do_parent(r);
    }
}
