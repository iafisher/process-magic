CC = gcc
CFLAGS = -Wall

all: out/waitforinput out/splitpoem

out/waitforinput: waitforinput.c
	$(CC) $(CFLAGS) -o $@ $<

out/splitpoem: splitpoem.c
	$(CC) $(CFLAGS) -o $@ $<
