CC = gcc
CFLAGS = -Wall -g

all: out/waitforinput out/splitpoem out/countforever out/takeover out/execve out/writetty out/mmap out/oblivion out/another_tty out/simple_malloc out/risen

out/%: %.c
	$(CC) $(CFLAGS) -o $@ $<
