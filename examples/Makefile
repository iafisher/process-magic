CC = gcc
CFLAGS = -Wall -g

all: out/waitforinput out/splitpoem out/countforever out/takeover out/execve out/writetty out/mmap out/obliterate out/another_tty

out/%: %.c
	$(CC) $(CFLAGS) -o $@ $<
