CC = gcc
CFLAGS = -Wall -g

all: out/waitforinput out/splitpoem out/countforever out/takeover out/execve out/writetty out/mmap

out/%: %.c
	$(CC) $(CFLAGS) -o $@ $<
