/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * cb-firstframe: boot-slot spike (#3). Sets a mode on the first connected
 * HDMI connector through raw DRM/KMS ioctls, fills the screen with the slot
 * colour (A = blue, B = green), prints a timing marker on the console and
 * keeps the frame on screen.
 */

#define _GNU_SOURCE

#include <drm/drm.h>
#include <drm/drm_mode.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define MAX_CARDS     8
#define WAIT_TOTAL_MS 15000
#define WAIT_STEP_MS  5

static int xioctl(int fd, unsigned long request, void *arg)
{
    int ret;

    do {
        ret = ioctl(fd, request, arg);
    } while (ret == -1 && (errno == EINTR || errno == EAGAIN));
    return ret;
}

static long boottime_ms(void)
{
    struct timespec ts;

    clock_gettime(CLOCK_BOOTTIME, &ts);
    return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}

static void sleep_ms(long ms)
{
    struct timespec ts = {.tv_sec = ms / 1000, .tv_nsec = (ms % 1000) * 1000000L};

    nanosleep(&ts, NULL);
}

/* Reads "cb.slot=<x>" from the kernel command line; defaults to "?". */
static char slot_from_cmdline(void)
{
    char buf[1024] = {0};
    FILE *f = fopen("/proc/cmdline", "r");
    char *p;

    if (f == NULL) {
        return '?';
    }
    if (fgets(buf, sizeof(buf), f) == NULL) {
        buf[0] = '\0';
    }
    fclose(f);
    p = strstr(buf, "cb.slot=");
    return p != NULL ? p[strlen("cb.slot=")] : '?';
}

struct output {
    uint32_t connector_id;
    uint32_t crtc_id;
    struct drm_mode_modeinfo mode;
};

/* Finds a connected connector with a mode and a CRTC that can drive it. */
static int find_output(int fd, struct output *out)
{
    struct drm_mode_card_res res = {0};
    uint32_t *conn_ids = NULL;
    uint32_t *crtc_ids = NULL;
    uint32_t *enc_ids = NULL;
    int found = -1;

    if (xioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res) != 0 || res.count_connectors == 0 || res.count_crtcs == 0) {
        return -1;
    }
    conn_ids = calloc(res.count_connectors, sizeof(*conn_ids));
    crtc_ids = calloc(res.count_crtcs, sizeof(*crtc_ids));
    enc_ids = calloc(res.count_encoders ? res.count_encoders : 1, sizeof(*enc_ids));
    if (conn_ids == NULL || crtc_ids == NULL || enc_ids == NULL) {
        goto done;
    }
    res.connector_id_ptr = (uint64_t)(uintptr_t)conn_ids;
    res.crtc_id_ptr = (uint64_t)(uintptr_t)crtc_ids;
    res.encoder_id_ptr = (uint64_t)(uintptr_t)enc_ids;
    res.count_fbs = 0;
    if (xioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res) != 0) {
        goto done;
    }

    for (uint32_t i = 0; i < res.count_connectors && found != 0; i++) {
        struct drm_mode_get_connector conn = {.connector_id = conn_ids[i]};
        struct drm_mode_modeinfo *modes;
        uint32_t *encoders;

        if (xioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &conn) != 0 || conn.connection != 1 || conn.count_modes == 0) {
            continue;
        }
        modes = calloc(conn.count_modes, sizeof(*modes));
        encoders = calloc(conn.count_encoders ? conn.count_encoders : 1, sizeof(*encoders));
        if (modes == NULL || encoders == NULL) {
            free(modes);
            free(encoders);
            continue;
        }
        conn.modes_ptr = (uint64_t)(uintptr_t)modes;
        conn.encoders_ptr = (uint64_t)(uintptr_t)encoders;
        conn.count_props = 0;
        if (xioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &conn) == 0) {
            out->mode = modes[0];
            for (uint32_t m = 0; m < conn.count_modes; m++) {
                if (modes[m].type & DRM_MODE_TYPE_PREFERRED) {
                    out->mode = modes[m];
                    break;
                }
            }
            for (uint32_t e = 0; e < conn.count_encoders && found != 0; e++) {
                struct drm_mode_get_encoder enc = {.encoder_id = encoders[e]};

                if (xioctl(fd, DRM_IOCTL_MODE_GETENCODER, &enc) != 0) {
                    continue;
                }
                for (uint32_t c = 0; c < res.count_crtcs; c++) {
                    if (enc.possible_crtcs & (1U << c)) {
                        out->connector_id = conn.connector_id;
                        out->crtc_id = crtc_ids[c];
                        found = 0;
                        break;
                    }
                }
            }
        }
        free(modes);
        free(encoders);
    }

done:
    free(conn_ids);
    free(crtc_ids);
    free(enc_ids);
    return found;
}

static int open_card_with_output(struct output *out)
{
    char path[32];

    for (int i = 0; i < MAX_CARDS; i++) {
        int fd;

        (void)snprintf(path, sizeof(path), "/dev/dri/card%d", i);
        fd = open(path, O_RDWR | O_CLOEXEC);
        if (fd < 0) {
            continue;
        }
        if (find_output(fd, out) == 0) {
            return fd;
        }
        close(fd);
    }
    return -1;
}

int main(void)
{
    struct output out = {0};
    char slot = slot_from_cmdline();
    uint32_t colour = slot == 'b' ? 0x0000a000U : 0x000040c0U;
    long waited = 0;
    int fd;

    while ((fd = open_card_with_output(&out)) < 0) {
        if (waited >= WAIT_TOTAL_MS) {
            printf("CB_FIRST_FRAME_FAILED slot=%c reason=no-connected-display boottime_ms=%ld\n", slot, boottime_ms());
            return 1;
        }
        sleep_ms(WAIT_STEP_MS);
        waited += WAIT_STEP_MS;
    }

    struct drm_mode_create_dumb create = {.width = out.mode.hdisplay, .height = out.mode.vdisplay, .bpp = 32};
    if (xioctl(fd, DRM_IOCTL_MODE_CREATE_DUMB, &create) != 0) {
        perror("CREATE_DUMB");
        return 1;
    }

    struct drm_mode_fb_cmd fb = {
        .width = create.width,
        .height = create.height,
        .pitch = create.pitch,
        .bpp = 32,
        .depth = 24,
        .handle = create.handle,
    };
    if (xioctl(fd, DRM_IOCTL_MODE_ADDFB, &fb) != 0) {
        perror("ADDFB");
        return 1;
    }

    struct drm_mode_map_dumb map = {.handle = create.handle};
    if (xioctl(fd, DRM_IOCTL_MODE_MAP_DUMB, &map) != 0) {
        perror("MAP_DUMB");
        return 1;
    }
    uint32_t *pixels = mmap(NULL, create.size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, (off_t)map.offset);
    if (pixels == MAP_FAILED) {
        perror("mmap");
        return 1;
    }
    for (uint64_t i = 0; i < create.size / sizeof(*pixels); i++) {
        pixels[i] = colour;
    }

    struct drm_mode_crtc crtc = {
        .crtc_id = out.crtc_id,
        .fb_id = fb.fb_id,
        .set_connectors_ptr = (uint64_t)(uintptr_t)&out.connector_id,
        .count_connectors = 1,
        .mode = out.mode,
        .mode_valid = 1,
    };
    if (xioctl(fd, DRM_IOCTL_MODE_SETCRTC, &crtc) != 0) {
        perror("SETCRTC");
        return 1;
    }

    printf("CB_FIRST_FRAME slot=%c mode=%ux%u boottime_ms=%ld\n", slot, out.mode.hdisplay, out.mode.vdisplay,
           boottime_ms());
    (void)fflush(stdout);

    /* Closing the DRM device would let the console take the display back. */
    for (;;) {
        pause();
    }
}
