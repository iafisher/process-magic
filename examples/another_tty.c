#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <unistd.h>

void do_child() {
    setpgid(0, 0);
    raise(SIGSTOP);
    puts("child exiting");
}

void wait_for_input() {
    puts("Press <Enter> to continue.");
    getc(stdin);
}

void do_parent(pid_t child) {
    printf("Parent PID: %d\n", getpid());
    printf("Child PID:  %d\n", child);

    waitpid(child, NULL, WSTOPPED);
    pid_t child_pgid = getpgid(child);

    int r1 = setpgid(0, child_pgid);
    if (r1 == -1) {
        perror("setpgid()");
        return;
    }

    pid_t r2 = setsid();
    if (r2 == -1) {
        perror("setsid()");
        return;
    }

    kill(child, SIGCONT);
    waitpid(child, NULL, 0);
    puts("waited for child to exit");

    const char* term = "/dev/pts/5";
    int fd = open(term, O_RDONLY, 0);
    if (fd == -1) {
        perror("open()");
        return;
    }

    int r3 = ioctl(fd, TIOCSCTTY, 1);
    if (r3 == -1) {
        perror("ioctl()");
        return;
    }

    close(fd);
    close(1);
    close(2);

    fd = open(term, O_WRONLY, 0);
    if (fd == -1) {
        perror("open() stdout");
        return;
    }

    fd = open(term, O_WRONLY, 0);
    if (fd == -1) {
        perror("open() stderr");
        return;
    }

    puts("success!");

    while (1) {
        puts("alive");
        sleep(1);
    }
}

int main(int argc, char* argv[]) {
    if (argc != 2) {
        fputs("error: expected one argument\n", stdout);
        return 1;
    }

    puts("fork()");
    pid_t r = fork();
    switch (r) {
        case -1:
            perror("fork");
            return 1;
        case 0:
            do_child();
            break;
        default:
            do_parent(r);
            break;
    }
}
