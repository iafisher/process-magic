#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>

void clear_screen() {
    printf("\033[2J");
}

void hide_cursor() {
    printf("\033[?25l");
}

void set_cursor(int row, int col) {
    printf("\033[%d;%dH", row, col);
}

void return_cursor() {
    set_cursor(1, 1);
}

void show_cursor() {
    printf("\033[?25h");
}

struct winsize get_terminal_size() {
    struct winsize r;
    ioctl(STDOUT_FILENO, TIOCGWINSZ, &r);
    return r;
}

#define MILLIS_IN_NANOS (1000*1000)

void sleep_ms(int ms) {
    struct timespec ts;
    ts.tv_sec = 0;
    ts.tv_nsec = ms * MILLIS_IN_NANOS;
    nanosleep(&ts, NULL);
}

void sleep_a_bit() {
    sleep_ms(100);
}

void paint_one_frame(struct winsize terminal_size) {
    clear_screen();
    return_cursor();
    for (int x = 0; x < terminal_size.ws_row; x++) {
        for (int y = 0; y < terminal_size.ws_col; y++) {
            if (rand() % 4 == 0) {
                switch (rand() % 4) {
                    case 0:
                        printf("*");
                    case 1:
                        printf("x");
                    case 2:
                        printf("-");
                    case 3:
                        printf("o");
                }
            } else {
                printf(" ");
            }
        }

        if (x != terminal_size.ws_row - 1) {
            printf("\n");
        }
    }
}

void fill_screen(struct winsize terminal_size, const char* s) {
    clear_screen();
    return_cursor();
    for (int x = 0; x < terminal_size.ws_row; x++) {
        for (int y = 0; y < terminal_size.ws_col; y++) {
            printf("%s", s);
        }

        if (x != terminal_size.ws_row - 1) {
            printf("\n");
        }
    }
}

void paint() {
    for (int i = 0; i < 10; i++) {
        switch (rand() % 3) {
            case 0:
                puts("segmentation fault");
                break;
            default:
                puts("system error");
        }
        sleep_ms(30);
    }

    // srand(time(NULL));

    // clear_screen();
    // hide_cursor();
    // struct winsize terminal_size = get_terminal_size();
    // for (int i = 0; i < 20; i++) {
    //     paint_one_frame(terminal_size);
    //     sleep_a_bit();
    // }
}

static int random_poem_counter = 0;
const char* poems[] = {
    "Summer surprised us, coming over the Starnbergersee",
    "Though much is taken, much abides",
    "Things fall apart; the centre cannot hold",
    NULL,
};

const char* random_poem() {
    const char* r = poems[random_poem_counter];
    if (r != NULL) {
        random_poem_counter++;
    }
    return r;
}

const char* random_error(int poem) {
    switch (rand() % (poem ? 6 : 5)) {
        case 0:
            return "system error";
        case 1:
            return "segmentation fault";
        case 2:
            return "reboot reboot reboot";
        case 3:
            return "core dumped";
        case 4:
            return "***********";
        default:
            return random_poem();
    }
}

// prints error messages at random locations on the screen
void animation1() {
    hide_cursor();
    clear_screen();
    struct winsize terminal_size = get_terminal_size();
    int ms = 400;
    for (int i = 0; i < 30; i++) {
        const char* msg = random_error(i >= 5);
        if (msg == NULL) {
            continue;
        }
        size_t n = strlen(msg);

        int row = rand() % terminal_size.ws_row;
        int col = rand() % (terminal_size.ws_col - (n - 1));
        set_cursor(row + 1, col + 1);
        printf("%s", msg);
        fflush(stdout);

        int jitter = (rand() % 40) - 20;
        sleep_ms(ms + jitter);
        if (ms >= 220) {
            ms -= 20;
        }
    }
}

// fills the screen with asterisks
void animation2() {
    clear_screen();
    return_cursor();
    struct winsize terminal_size = get_terminal_size();

    const char* msg = "  no more computer  ";
    size_t n = strlen(msg);
    int target_x = terminal_size.ws_row / 2;
    int target_y = (terminal_size.ws_col / 2) - (n / 2);
    for (int x = 0; x < terminal_size.ws_row; x++) {
        for (int y = 0; y < terminal_size.ws_col; y++) {
            char c;
            if (x == target_x && y >= target_y && y < target_y + n) {
                c = msg[y - target_y];
            } else {
                c = '*';
            }

            printf("%c", c);
            fflush(stdout);
            sleep_ms(1);
        }

        if (x != terminal_size.ws_row - 1) {
            printf("\n");
        }
    }
}

void animation3() {
    clear_screen();
    hide_cursor();
    struct winsize terminal_size = get_terminal_size();
    int x1 = 1;
    int x2 = terminal_size.ws_col;
    int y1 = 1;
    int y2 = terminal_size.ws_row;
    int i = 0;

    while (!(y1 == y2 && x2 - x1 < 0)) {
        if (i % 2 == 0) {
            set_cursor(y1, x1);
            if (x1 == terminal_size.ws_col) {
                x1 = 1;
                y1++;
            } else {
                x1++;
            }
        } else {
            set_cursor(y2, x2);
            if (x2 == 0) {
                x2 = terminal_size.ws_col;
                y2--;
            } else {
                x2--;
            }
        }

        printf("*");
        fflush(stdout);
        sleep_ms(1);
        i++;
    }

    // while (!(y1 == y2 && x2 - x1 <= 1)) {
    //     if (i % 2 == 0) {
    //         set_cursor(x1, y1);
    //         printf("*");

    //         if (x1 == terminal_size.ws_col) {
    //             x1 = 1;
    //             y1++;
    //         } else {
    //             x1++;
    //         }
    //     } else {
    //         set_cursor(x2, y2);
    //         printf("*");

    //         if (x2 == 0) {
    //             x2 = terminal_size.ws_col;
    //             y2--;
    //         } else {
    //             x2--;
    //         }
    //     }
    //     fflush(stdout);
    //     sleep_ms(1);

    //     i++;
    // }
}

int main(int argc, char* argv[]) {
    char* selection;
    if (argc >= 2) {
        selection = argv[1];
    } else {
        selection = "primary";
    }

    if (strcmp(selection, "secondary") == 0) {
        animation1();
    } else {
        animation3();
    }

    getc(stdin);
    clear_screen();
    return_cursor();
    show_cursor();

    return 0;
}
