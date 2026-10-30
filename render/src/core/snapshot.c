/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include "internal.h"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define SNAPSHOT_MAGIC 0x31504e534243ULL /* "CBSNP1" */
#define IMAGE_MAGIC    0x31474d494243ULL /* "CBIMG1" */

typedef struct {
    uint64_t magic;
    uint64_t payload_checksum;
    uint64_t payload_size;
} cb_file_header_t;

typedef struct {
    uint64_t magic;
    uint32_t width;
    uint32_t height;
    uint64_t checksum;
} cb_image_header_t;

static uint64_t compute_checksum(const uint8_t *bytes, size_t len)
{
    uint64_t h = 0xcbf29ce484222325ULL;

    for (size_t i = 0; i < len; i++) {
        h ^= (uint64_t)bytes[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

static bool is_valid_hash_str(const char *s)
{
    size_t len;

    if (s == NULL) {
        return false;
    }
    len = strlen(s);
    if (len == 0U || len >= CB_MAX_HASH_HEX) {
        return false;
    }
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F') || c == '-' || c == '_')) {
            return false;
        }
    }
    return true;
}

static cb_render_err_t ensure_dir(const char *path)
{
    if (mkdir(path, 0755) == 0 || errno == EEXIST) {
        return CB_RENDER_OK;
    }
    return CB_RENDER_ERR_IO;
}

static cb_render_err_t fsync_directory(const char *dir_path)
{
    int dir_fd = open(dir_path, O_RDONLY);
    int sync_rc;

    if (dir_fd < 0) {
        return CB_RENDER_ERR_IO;
    }
    sync_rc = fsync(dir_fd);
    (void)close(dir_fd);
    return (sync_rc == 0) ? CB_RENDER_OK : CB_RENDER_ERR_IO;
}

static cb_render_err_t write_all_fd(int fd, const uint8_t *data, size_t len)
{
    size_t written = 0U;

    while (written < len) {
        ssize_t n = write(fd, data + written, len - written);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return CB_RENDER_ERR_IO;
        }
        written += (size_t)n;
    }
    return CB_RENDER_OK;
}

static cb_render_err_t read_all_fd(int fd, uint8_t *data, size_t len)
{
    size_t total = 0U;

    while (total < len) {
        ssize_t n = read(fd, data + total, len - total);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return CB_RENDER_ERR_IO;
        }
        if (n == 0) {
            return CB_RENDER_ERR_CORRUPT;
        }
        total += (size_t)n;
    }
    return CB_RENDER_OK;
}

cb_render_err_t cb_write_file_atomic(const char *dir_path, const char *file_name, const uint8_t *data, size_t len)
{
    char tmp_path[CB_MAX_PATH_LEN];
    char final_path[CB_MAX_PATH_LEN];
    int fd;
    int n_tmp;
    int n_final;
    cb_render_err_t err;

    if (dir_path == NULL || file_name == NULL || (data == NULL && len > 0U)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (ensure_dir(dir_path) != CB_RENDER_OK) {
        return CB_RENDER_ERR_IO;
    }
    n_tmp = snprintf(tmp_path, sizeof(tmp_path), "%s/.%s.tmp.%ld", dir_path, file_name, (long)getpid());
    n_final = snprintf(final_path, sizeof(final_path), "%s/%s", dir_path, file_name);
    if (n_tmp < 0 || (size_t)n_tmp >= sizeof(tmp_path) || n_final < 0 || (size_t)n_final >= sizeof(final_path)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }

    fd = open(tmp_path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        return CB_RENDER_ERR_IO;
    }
    err = write_all_fd(fd, data, len);
    if (err == CB_RENDER_OK && fsync(fd) != 0) {
        err = CB_RENDER_ERR_IO;
    }
    if (close(fd) != 0 && err == CB_RENDER_OK) {
        err = CB_RENDER_ERR_IO;
    }
    if (err != CB_RENDER_OK) {
        (void)unlink(tmp_path);
        return err;
    }
    if (rename(tmp_path, final_path) != 0) {
        (void)unlink(tmp_path);
        return CB_RENDER_ERR_IO;
    }
    return fsync_directory(dir_path);
}

cb_render_err_t cb_snapshot_save_to_store(const char *snapshots_root, const cb_snapshot_t *snapshot)
{
    char by_hash_dir[CB_MAX_PATH_LEN];
    char snap_dir[CB_MAX_PATH_LEN];
    cb_file_header_t hdr;
    uint8_t *buf;
    size_t total_len;
    int n1;
    int n2;
    cb_render_err_t err;

    if (snapshots_root == NULL || snapshot == NULL || !is_valid_hash_str(snapshot->sha256_hex)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (snapshot->schema_version != 1U || snapshot->page_count == 0U || snapshot->page_count > CB_MAX_PAGES) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (ensure_dir(snapshots_root) != CB_RENDER_OK) {
        return CB_RENDER_ERR_IO;
    }
    n1 = snprintf(by_hash_dir, sizeof(by_hash_dir), "%s/by-hash", snapshots_root);
    n2 = snprintf(snap_dir, sizeof(snap_dir), "%s/by-hash/%s", snapshots_root, snapshot->sha256_hex);
    if (n1 < 0 || (size_t)n1 >= sizeof(by_hash_dir) || n2 < 0 || (size_t)n2 >= sizeof(snap_dir)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (ensure_dir(by_hash_dir) != CB_RENDER_OK || ensure_dir(snap_dir) != CB_RENDER_OK) {
        return CB_RENDER_ERR_IO;
    }

    hdr.magic = SNAPSHOT_MAGIC;
    hdr.payload_size = (uint64_t)sizeof(cb_snapshot_t);
    hdr.payload_checksum = compute_checksum((const uint8_t *)snapshot, sizeof(cb_snapshot_t));

    total_len = sizeof(hdr) + sizeof(cb_snapshot_t);
    buf = malloc(total_len);
    if (buf == NULL) {
        return CB_RENDER_ERR_NOMEM;
    }
    memcpy(buf, &hdr, sizeof(hdr));
    memcpy(buf + sizeof(hdr), snapshot, sizeof(cb_snapshot_t));

    err = cb_write_file_atomic(snap_dir, "snapshot.cb", buf, total_len);
    free(buf);
    return err;
}

cb_render_err_t cb_snapshot_set_pointer_atomic(const char *snapshots_root, const char *pointer_name,
                                               const char *sha256_hex)
{
    char line[CB_MAX_HASH_HEX + 2U];
    int n;

    if (snapshots_root == NULL || pointer_name == NULL || !is_valid_hash_str(sha256_hex)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    n = snprintf(line, sizeof(line), "%s\n", sha256_hex);
    if (n < 0 || (size_t)n >= sizeof(line)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    return cb_write_file_atomic(snapshots_root, pointer_name, (const uint8_t *)line, (size_t)n);
}

cb_render_err_t cb_snapshot_read_pointer(const char *snapshots_root, const char *pointer_name, char *out_hash,
                                         size_t out_size)
{
    char path[CB_MAX_PATH_LEN];
    char buf[CB_MAX_HASH_HEX + 8U];
    int fd;
    ssize_t n_read;
    size_t len = 0U;
    int n_path;

    if (snapshots_root == NULL || pointer_name == NULL || out_hash == NULL || out_size < CB_MAX_HASH_HEX) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    n_path = snprintf(path, sizeof(path), "%s/%s", snapshots_root, pointer_name);
    if (n_path < 0 || (size_t)n_path >= sizeof(path)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    fd = open(path, O_RDONLY);
    if (fd < 0) {
        return CB_RENDER_ERR_NO_SNAPSHOT;
    }
    n_read = read(fd, buf, sizeof(buf) - 1U);
    (void)close(fd);
    if (n_read <= 0) {
        return CB_RENDER_ERR_CORRUPT;
    }
    buf[n_read] = '\0';
    while (len < (size_t)n_read && buf[len] != '\n' && buf[len] != '\r' && buf[len] != ' ' && buf[len] != '\0') {
        len++;
    }
    if (len == 0U || len >= out_size) {
        return CB_RENDER_ERR_CORRUPT;
    }
    memcpy(out_hash, buf, len);
    out_hash[len] = '\0';
    if (!is_valid_hash_str(out_hash)) {
        return CB_RENDER_ERR_CORRUPT;
    }
    return CB_RENDER_OK;
}

static bool validate_loaded_snapshot(const cb_snapshot_t *snap, const char *expected_hash)
{
    if (snap->schema_version != 1U || snap->page_count == 0U || snap->page_count > CB_MAX_PAGES) {
        return false;
    }
    if (snap->logical_width == 0U || snap->logical_height == 0U || snap->logical_width > CB_4K_WIDTH ||
        snap->logical_height > CB_4K_HEIGHT) {
        return false;
    }
    if (strcmp(snap->sha256_hex, expected_hash) != 0) {
        return false;
    }
    for (uint32_t i = 0U; i < snap->page_count; i++) {
        if (snap->pages[i].item_count > CB_MAX_ITEMS) {
            return false;
        }
    }
    return true;
}

cb_render_err_t cb_snapshot_load_by_hash(const char *snapshots_root, const char *sha256_hex, cb_snapshot_t *out_snap)
{
    char path[CB_MAX_PATH_LEN];
    cb_file_header_t hdr;
    int fd;
    int n_path;
    cb_render_err_t err;
    uint64_t actual_checksum;

    if (snapshots_root == NULL || !is_valid_hash_str(sha256_hex) || out_snap == NULL) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    n_path = snprintf(path, sizeof(path), "%s/by-hash/%s/snapshot.cb", snapshots_root, sha256_hex);
    if (n_path < 0 || (size_t)n_path >= sizeof(path)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    fd = open(path, O_RDONLY);
    if (fd < 0) {
        return CB_RENDER_ERR_NO_SNAPSHOT;
    }
    err = read_all_fd(fd, (uint8_t *)&hdr, sizeof(hdr));
    if (err != CB_RENDER_OK || hdr.magic != SNAPSHOT_MAGIC || hdr.payload_size != sizeof(cb_snapshot_t)) {
        (void)close(fd);
        return CB_RENDER_ERR_CORRUPT;
    }
    err = read_all_fd(fd, (uint8_t *)out_snap, sizeof(cb_snapshot_t));
    (void)close(fd);
    if (err != CB_RENDER_OK) {
        return err;
    }
    actual_checksum = compute_checksum((const uint8_t *)out_snap, sizeof(cb_snapshot_t));
    if (actual_checksum != hdr.payload_checksum || !validate_loaded_snapshot(out_snap, sha256_hex)) {
        return CB_RENDER_ERR_CORRUPT;
    }
    return CB_RENDER_OK;
}

cb_render_err_t cb_render_cache_save_image(const char *snapshots_root, const char *snapshot_hash,
                                           const char *image_hash, uint32_t width, uint32_t height,
                                           const uint32_t *pixels)
{
    char snap_dir[CB_MAX_PATH_LEN];
    char cache_dir[CB_MAX_PATH_LEN];
    char file_name[CB_MAX_HASH_HEX + 8U];
    cb_image_header_t hdr;
    size_t pixel_bytes;
    uint8_t *buf;
    int n1;
    int n2;
    int n3;
    cb_render_err_t err;

    if (snapshots_root == NULL || !is_valid_hash_str(snapshot_hash) || !is_valid_hash_str(image_hash) ||
        pixels == NULL || width == 0U || height == 0U || width > CB_4K_WIDTH || height > CB_4K_HEIGHT) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    n1 = snprintf(snap_dir, sizeof(snap_dir), "%s/by-hash/%s", snapshots_root, snapshot_hash);
    n2 = snprintf(cache_dir, sizeof(cache_dir), "%s/by-hash/%s/cache", snapshots_root, snapshot_hash);
    n3 = snprintf(file_name, sizeof(file_name), "%s.cimg", image_hash);
    if (n1 < 0 || (size_t)n1 >= sizeof(snap_dir) || n2 < 0 || (size_t)n2 >= sizeof(cache_dir) || n3 < 0 ||
        (size_t)n3 >= sizeof(file_name)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (ensure_dir(snap_dir) != CB_RENDER_OK || ensure_dir(cache_dir) != CB_RENDER_OK) {
        return CB_RENDER_ERR_IO;
    }

    pixel_bytes = (size_t)width * (size_t)height * sizeof(uint32_t);
    hdr.magic = IMAGE_MAGIC;
    hdr.width = width;
    hdr.height = height;
    hdr.checksum = compute_checksum((const uint8_t *)pixels, pixel_bytes);

    buf = malloc(sizeof(hdr) + pixel_bytes);
    if (buf == NULL) {
        return CB_RENDER_ERR_NOMEM;
    }
    memcpy(buf, &hdr, sizeof(hdr));
    memcpy(buf + sizeof(hdr), pixels, pixel_bytes);

    err = cb_write_file_atomic(cache_dir, file_name, buf, sizeof(hdr) + pixel_bytes);
    free(buf);
    return err;
}

cb_render_err_t cb_render_cache_load_image(const char *snapshots_root, const char *snapshot_hash,
                                           const char *image_hash, cb_cached_image_t *out_img)
{
    char path[CB_MAX_PATH_LEN];
    cb_image_header_t hdr;
    size_t pixel_bytes;
    uint32_t *pixels;
    int fd;
    int n_path;
    cb_render_err_t err;

    if (snapshots_root == NULL || !is_valid_hash_str(snapshot_hash) || !is_valid_hash_str(image_hash) ||
        out_img == NULL) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    n_path = snprintf(path, sizeof(path), "%s/by-hash/%s/cache/%s.cimg", snapshots_root, snapshot_hash, image_hash);
    if (n_path < 0 || (size_t)n_path >= sizeof(path)) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    fd = open(path, O_RDONLY);
    if (fd < 0) {
        return CB_RENDER_ERR_NO_SNAPSHOT;
    }
    err = read_all_fd(fd, (uint8_t *)&hdr, sizeof(hdr));
    if (err != CB_RENDER_OK || hdr.magic != IMAGE_MAGIC || hdr.width == 0U || hdr.height == 0U ||
        hdr.width > CB_4K_WIDTH || hdr.height > CB_4K_HEIGHT) {
        (void)close(fd);
        return CB_RENDER_ERR_CORRUPT;
    }
    pixel_bytes = (size_t)hdr.width * (size_t)hdr.height * sizeof(uint32_t);
    pixels = malloc(pixel_bytes);
    if (pixels == NULL) {
        (void)close(fd);
        return CB_RENDER_ERR_NOMEM;
    }
    err = read_all_fd(fd, (uint8_t *)pixels, pixel_bytes);
    (void)close(fd);
    if (err != CB_RENDER_OK || compute_checksum((const uint8_t *)pixels, pixel_bytes) != hdr.checksum) {
        free(pixels);
        return CB_RENDER_ERR_CORRUPT;
    }
    memset(out_img, 0, sizeof(*out_img));
    (void)snprintf(out_img->sha256_hex, sizeof(out_img->sha256_hex), "%s", image_hash);
    out_img->width = hdr.width;
    out_img->height = hdr.height;
    out_img->pixels = pixels;
    return CB_RENDER_OK;
}

void cb_cached_image_free(cb_cached_image_t *img)
{
    if (img == NULL) {
        return;
    }
    free(img->pixels);
    img->pixels = NULL;
    img->width = 0U;
    img->height = 0U;
    img->sha256_hex[0] = '\0';
}
