# SPDX-License-Identifier: MIT OR Apache-2.0
CFLAGS ?= -O2
CFLAGS += -std=c11 -Wall -Wextra -Werror

all: cb-firstframe cb-tryboot-reboot cb-rawpatch

cb-firstframe: firstframe.c
	$(CC) $(CFLAGS) -o $@ $<

cb-tryboot-reboot: tryboot-reboot.c
	$(CC) $(CFLAGS) -o $@ $<

cb-rawpatch: rawpatch.c
	$(CC) $(CFLAGS) -o $@ $<

clean:
	rm -f cb-firstframe cb-tryboot-reboot cb-rawpatch

.PHONY: all clean
