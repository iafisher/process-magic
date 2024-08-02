#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main() {
    pid_t pid = getpid();
    printf("My PID: %d\n", pid);

    char* pathname = "/home/ian/proctool/bin/takeover";
    char* args[] = {pathname};
    char* envp[] = {NULL};
    int x = execve(pathname, args, envp);
    printf("execve returned %d\n", x);
    printf("errno: %s\n", strerror(errno));
    getc(stdin);
    return 0;
}
