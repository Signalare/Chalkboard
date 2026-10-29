/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * cb-rawpatch: power-loss-safe slot commit for the boot-slot spike (#3).
 *
 * Overwrites a few bytes of a file on a read-only mounted FAT filesystem by
 * writing directly to the underlying block device, so no filesystem metadata
 * (directory entry, timestamps, FAT, cluster allocation) is ever touched.
 * All patched bytes must lie in one 512-byte sector, which the device writes
 * in a single operation: after a power cut it holds either the old or the
 * new contents.
 *
 * Usage: cb-rawpatch DEVICE FILE OFFSET CHAR [OFFSET CHAR ...]
 *   DEVICE  partition holding FILE, e.g. /dev/mmcblk0p1
 *   FILE    path of the file on the mounted (read-only) filesystem
 *   OFFSET  byte offset within FILE; CHAR the single character to write
 */

#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <linux/fs.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

#define SECTOR    512
#define MAX_PATCH 8

struct patch {
    long file_off;
    char value;
    off_t dev_off;
};

/* Maps a byte offset in FILE to a byte offset on its block device. */
static int map_offset(int fd, long file_off, off_t *dev_off)
{
    int bsz = 0;
    int block;

    if (ioctl(fd, FIGETBSZ, &bsz) != 0 || bsz <= 0) {
        perror("FIGETBSZ");
        return -1;
    }
    block = (int)(file_off / bsz);
    if (ioctl(fd, FIBMAP, &block) != 0 || block == 0) {
        perror("FIBMAP");
        return -1;
    }
    *dev_off = (off_t)block * bsz + file_off % bsz;
    return 0;
}

int main(int argc, char **argv)
{
    struct patch patches[MAX_PATCH];
    int count = (argc - 3) / 2;
    char sector[SECTOR];
    off_t first;
    int fd;
    int dev;

    if (argc < 5 || (argc - 3) % 2 != 0 || count > MAX_PATCH) {
        (void)fputs("usage: cb-rawpatch DEVICE FILE OFFSET CHAR [OFFSET CHAR ...]\n", stderr);
        return 2;
    }

    fd = open(argv[2], O_RDONLY | O_CLOEXEC);
    if (fd < 0) {
        perror(argv[2]);
        return 1;
    }
    for (int i = 0; i < count; i++) {
        patches[i].file_off = strtol(argv[3 + 2 * i], NULL, 10);
        patches[i].value = argv[4 + 2 * i][0];
        if (map_offset(fd, patches[i].file_off, &patches[i].dev_off) != 0) {
            return 1;
        }
    }
    close(fd);

    first = patches[0].dev_off - patches[0].dev_off % SECTOR;
    for (int i = 1; i < count; i++) {
        if (patches[i].dev_off - patches[i].dev_off % SECTOR != first) {
            (void)fputs("cb-rawpatch: patched bytes span more than one sector\n", stderr);
            return 1;
        }
    }

    dev = open(argv[1], O_RDWR | O_CLOEXEC | O_SYNC);
    if (dev < 0) {
        perror(argv[1]);
        return 1;
    }
    if (pread(dev, sector, SECTOR, first) != SECTOR) {
        perror("pread");
        return 1;
    }
    for (int i = 0; i < count; i++) {
        sector[patches[i].dev_off - first] = patches[i].value;
    }
    /* One aligned whole-sector write: old or new contents, never a mix. */
    if (pwrite(dev, sector, SECTOR, first) != SECTOR) {
        perror("pwrite");
        return 1;
    }
    if (fsync(dev) != 0) {
        perror("fsync");
        return 1;
    }
    close(dev);
    return 0;
}
