/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * cb-growpart: First-boot partition growth for Chalkboard (STO-008, STO-009).
 *
 * Safely grows an MBR logical partition (specifically partition 7, /data)
 * up to 90% of disk capacity, capped at 32 GB (67,108,864 sectors).
 * Updates both the EBR logical partition size and the MBR extended partition container.
 * Idempotent: safe no-op if already grown.
 */

#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fs.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

#define SECTOR_SIZE       512
#define MAX_SECTORS_32GB  67108864ULL  /* 32 GB in 512-byte sectors */
#define TOLERANCE_SECTORS 2048ULL      /* 1 MB alignment tolerance */

static uint32_t read_le32(const uint8_t *p)
{
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static void write_le32(uint8_t *p, uint32_t v)
{
    p[0] = (uint8_t)(v & 0xff);
    p[1] = (uint8_t)((v >> 8) & 0xff);
    p[2] = (uint8_t)((v >> 16) & 0xff);
    p[3] = (uint8_t)((v >> 24) & 0xff);
}

static int read_sysfs_uint64(const char *path, uint64_t *val)
{
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    if (fscanf(f, "%" SCNu64, val) != 1) {
        fclose(f);
        return -1;
    }
    fclose(f);
    return 0;
}

int main(int argc, char **argv)
{
    const char *data_dev = (argc > 1) ? argv[1] : "/dev/sda7";
    const char *part_name = NULL;
    char sys_part[128], sys_disk[128], disk_dev[64];
    uint64_t part_start = 0, part_sectors = 0, disk_sectors = 0;
    uint64_t max_end = 0, available = 0, target_sectors = 0;
    uint8_t mbr[SECTOR_SIZE], ebr[SECTOR_SIZE];
    int fd = -1;

    if (strncmp(data_dev, "/dev/", 5) == 0) {
        part_name = data_dev + 5;
    } else {
        part_name = data_dev;
    }

    snprintf(sys_part, sizeof(sys_part), "/sys/class/block/%s", part_name);
    char part_start_path[160], part_size_path[160];
    snprintf(part_start_path, sizeof(part_start_path), "%s/start", sys_part);
    snprintf(part_size_path, sizeof(part_size_path), "%s/size", sys_part);

    if (read_sysfs_uint64(part_start_path, &part_start) != 0 ||
        read_sysfs_uint64(part_size_path, &part_sectors) != 0) {
        fprintf(stderr, "cb-growpart: cannot read partition sysfs for %s\n", data_dev);
        return 1;
    }

    /* Determine disk name (e.g. sda7 -> sda, mmcblk0p7 -> mmcblk0) */
    char disk_name[32] = {0};
    if (strncmp(part_name, "mmcblk", 6) == 0 || strncmp(part_name, "nvme", 4) == 0) {
        const char *p = strrchr(part_name, 'p');
        if (p) {
            size_t len = (size_t)(p - part_name);
            strncpy(disk_name, part_name, len);
        }
    } else {
        /* e.g. sda7 -> sda */
        size_t len = strlen(part_name);
        while (len > 0 && part_name[len - 1] >= '0' && part_name[len - 1] <= '9') len--;
        strncpy(disk_name, part_name, len);
    }

    snprintf(sys_disk, sizeof(sys_disk), "/sys/class/block/%s/size", disk_name);
    if (read_sysfs_uint64(sys_disk, &disk_sectors) != 0) {
        fprintf(stderr, "cb-growpart: cannot read disk size for %s\n", disk_name);
        return 1;
    }

    snprintf(disk_dev, sizeof(disk_dev), "/dev/%s", disk_name);

    /* 10% spare area unpartitioned (STO-009) */
    max_end = (disk_sectors * 90ULL) / 100ULL;
    if (max_end <= part_start) {
        printf("cb-growpart: disk too small to leave 10%% spare area\n");
        return 0;
    }

    available = max_end - part_start;
    target_sectors = (available < MAX_SECTORS_32GB) ? available : MAX_SECTORS_32GB;

    printf("cb-growpart: %s start=%" PRIu64 " current=%" PRIu64 " target=%" PRIu64 " disk_total=%" PRIu64 "\n",
           data_dev, part_start, part_sectors, target_sectors, disk_sectors);

    if (target_sectors <= part_sectors || (target_sectors - part_sectors) <= TOLERANCE_SECTORS) {
        printf("cb-growpart: partition %s is already at target size (%" PRIu64 " sectors). No change needed.\n",
               data_dev, part_sectors);
        return 0;
    }

    fd = open(disk_dev, O_RDWR | O_SYNC);
    if (fd < 0) {
        perror("cb-growpart: open disk");
        return 1;
    }

    /* Read MBR */
    if (pread(fd, mbr, SECTOR_SIZE, 0) != SECTOR_SIZE) {
        perror("cb-growpart: pread MBR");
        close(fd);
        return 1;
    }

    if (mbr[510] != 0x55 || mbr[511] != 0xaa) {
        fprintf(stderr, "cb-growpart: invalid MBR signature\n");
        close(fd);
        return 1;
    }

    /* Locate extended partition in MBR (partition 4: offset 0x1ee) */
    uint32_t ext_start = read_le32(&mbr[0x1ee + 8]);
    uint8_t ext_type = mbr[0x1ee + 4];

    if (ext_type != 0x05 && ext_type != 0x0f) {
        fprintf(stderr, "cb-growpart: partition 4 is not extended partition (type 0x%02x)\n", ext_type);
        close(fd);
        return 1;
    }

    /* EBR for partition 7 is at (part_start - 8192) sectors */
    uint64_t ebr_lba = part_start - 8192ULL;
    if (pread(fd, ebr, SECTOR_SIZE, (off_t)ebr_lba * SECTOR_SIZE) != SECTOR_SIZE) {
        perror("cb-growpart: pread EBR");
        close(fd);
        return 1;
    }

    if (ebr[510] != 0x55 || ebr[511] != 0xaa) {
        fprintf(stderr, "cb-growpart: invalid EBR signature at sector %" PRIu64 "\n", ebr_lba);
        close(fd);
        return 1;
    }

    /* Update EBR partition size */
    write_le32(&ebr[0x1be + 12], (uint32_t)target_sectors);
    if (pwrite(fd, ebr, SECTOR_SIZE, (off_t)ebr_lba * SECTOR_SIZE) != SECTOR_SIZE) {
        perror("cb-growpart: pwrite EBR");
        close(fd);
        return 1;
    }

    /* Update MBR extended partition size */
    uint32_t new_ext_sectors = (uint32_t)((part_start - ext_start) + target_sectors);
    write_le32(&mbr[0x1ee + 12], new_ext_sectors);
    if (pwrite(fd, mbr, SECTOR_SIZE, 0) != SECTOR_SIZE) {
        perror("cb-growpart: pwrite MBR");
        close(fd);
        return 1;
    }

    fdatasync(fd);

    printf("cb-growpart: updated EBR sector %" PRIu64 " and MBR sector 0. Rereading partition table...\n", ebr_lba);

    /* Reread partition table */
    if (ioctl(fd, BLKRRPART) != 0) {
        perror("cb-growpart: ioctl BLKRRPART");
    } else {
        printf("cb-growpart: BLKRRPART successful.\n");
    }

    close(fd);
    return 0;
}
