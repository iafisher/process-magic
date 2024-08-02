CC = gcc
CFLAGS = -Wall -g

all: out/waitforinput out/splitpoem out/countforever out/takeover out/execve

out/waitforinput: waitforinput.c
	$(CC) $(CFLAGS) -o $@ $<

out/splitpoem: splitpoem.c
	$(CC) $(CFLAGS) -o $@ $<

out/countforever: countforever.c
	$(CC) $(CFLAGS) -o $@ $<

out/takeover: takeover.c
	$(CC) $(CFLAGS) -o $@ $<

out/execve: execve.c
	$(CC) $(CFLAGS) -o $@ $<
