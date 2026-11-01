/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * chalkboard-render: draws the current snapshot via DRM/KMS (SPEC §7, RND-001).
 */

#ifdef __linux__
#include <drm/drm.h>
#include <drm/drm_mode.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#endif

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include "chalkboard/render.h"

#ifdef __linux__
typedef struct {
    int fd;
    uint32_t connector_id;
    uint32_t crtc_id;
    uint32_t fb_id;
    uint32_t handle;
    uint32_t pitch;
    uint64_t size;
    uint32_t width;
    uint32_t height;
    struct drm_mode_modeinfo mode;
    uint32_t *map;
} cb_drm_output_t;

static int drm_xioctl(int fd, unsigned long request, void *arg)
{
    int ret;

    do {
        ret = ioctl(fd, request, arg);
    } while (ret == -1 && (errno == EINTR || errno == EAGAIN));
    return ret;
}

static int find_crtc_for_encoder(int fd, uint32_t enc_id, const uint32_t *crtc_ids, uint32_t count_crtcs,
                                 uint32_t *out_crtc)
{
    struct drm_mode_get_encoder enc = {.encoder_id = enc_id};
    if (drm_xioctl(fd, DRM_IOCTL_MODE_GETENCODER, &enc) != 0) {
        return -1;
    }
    for (uint32_t c = 0; c < count_crtcs; c++) {
        if (enc.possible_crtcs & (1U << c)) {
            *out_crtc = crtc_ids[c];
            return 0;
        }
    }
    return -1;
}

static int try_connector(int fd, uint32_t conn_id, const uint32_t *crtc_ids, uint32_t count_crtcs, cb_drm_output_t *drm)
{
    struct drm_mode_get_connector conn = {.connector_id = conn_id};
    struct drm_mode_modeinfo *modes = NULL;
    uint32_t *encoders = NULL;
    int ret = -1;

    if (drm_xioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &conn) != 0 || conn.connection != 1 || conn.count_modes == 0) {
        return -1;
    }
    modes = (struct drm_mode_modeinfo *)calloc(conn.count_modes, sizeof(*modes));
    encoders = (uint32_t *)calloc(conn.count_encoders ? conn.count_encoders : 1U, sizeof(*encoders));
    if (modes == NULL || encoders == NULL) {
        free(modes);
        free(encoders);
        return -1;
    }
    conn.modes_ptr = (uint64_t)(uintptr_t)modes;
    conn.encoders_ptr = (uint64_t)(uintptr_t)encoders;
    if (drm_xioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &conn) == 0) {
        drm->mode = modes[0];
        for (uint32_t m = 0; m < conn.count_modes; m++) {
            if (modes[m].type & DRM_MODE_TYPE_PREFERRED) {
                drm->mode = modes[m];
                break;
            }
        }
        for (uint32_t e = 0; e < conn.count_encoders; e++) {
            if (find_crtc_for_encoder(fd, encoders[e], crtc_ids, count_crtcs, &drm->crtc_id) == 0) {
                drm->connector_id = conn.connector_id;
                ret = 0;
                break;
            }
        }
    }
    free(modes);
    free(encoders);
    return ret;
}

static int drm_find_output(int fd, cb_drm_output_t *drm)
{
    struct drm_mode_card_res res = {0};
    uint32_t *conn_ids = NULL;
    uint32_t *crtc_ids = NULL;
    int found = -1;

    if (drm_xioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res) != 0 || res.count_connectors == 0 || res.count_crtcs == 0) {
        return -1;
    }
    conn_ids = (uint32_t *)calloc(res.count_connectors, sizeof(*conn_ids));
    crtc_ids = (uint32_t *)calloc(res.count_crtcs, sizeof(*crtc_ids));
    if (conn_ids == NULL || crtc_ids == NULL) {
        free(conn_ids);
        free(crtc_ids);
        return -1;
    }
    res.connector_id_ptr = (uint64_t)(uintptr_t)conn_ids;
    res.crtc_id_ptr = (uint64_t)(uintptr_t)crtc_ids;
    if (drm_xioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res) == 0) {
        for (uint32_t i = 0; i < res.count_connectors; i++) {
            if (try_connector(fd, conn_ids[i], crtc_ids, res.count_crtcs, drm) == 0) {
                found = 0;
                break;
            }
        }
    }
    free(conn_ids);
    free(crtc_ids);
    return found;
}

static int drm_setup_dumb_fb(cb_drm_output_t *drm)
{
    struct drm_mode_create_dumb create = {
        .width = drm->mode.hdisplay,
        .height = drm->mode.vdisplay,
        .bpp = 32U,
    };
    struct drm_mode_fb_cmd fb = {0};
    struct drm_mode_map_dumb map_req = {0};
    struct drm_mode_crtc crtc = {0};
    void *mapped;

    if (drm_xioctl(drm->fd, DRM_IOCTL_MODE_CREATE_DUMB, &create) != 0) {
        return -1;
    }
    drm->handle = create.handle;
    drm->pitch = create.pitch;
    drm->size = create.size;
    drm->width = create.width;
    drm->height = create.height;

    fb.width = create.width;
    fb.height = create.height;
    fb.pitch = create.pitch;
    fb.bpp = 32U;
    fb.depth = 24U;
    fb.handle = create.handle;
    if (drm_xioctl(drm->fd, DRM_IOCTL_MODE_ADDFB, &fb) != 0) {
        return -1;
    }
    drm->fb_id = fb.fb_id;

    map_req.handle = create.handle;
    if (drm_xioctl(drm->fd, DRM_IOCTL_MODE_MAP_DUMB, &map_req) != 0) {
        return -1;
    }
    mapped = mmap(NULL, (size_t)create.size, PROT_READ | PROT_WRITE, MAP_SHARED, drm->fd, (off_t)map_req.offset);
    if (mapped == MAP_FAILED) {
        return -1;
    }
    drm->map = (uint32_t *)mapped;

    crtc.crtc_id = drm->crtc_id;
    crtc.fb_id = drm->fb_id;
    crtc.set_connectors_ptr = (uint64_t)(uintptr_t)&drm->connector_id;
    crtc.count_connectors = 1U;
    crtc.mode = drm->mode;
    crtc.mode_valid = 1U;
    return drm_xioctl(drm->fd, DRM_IOCTL_MODE_SETCRTC, &crtc);
}

static void drm_present_surface(cb_drm_output_t *drm, const cb_surface_t *surface)
{
    uint32_t copy_w;
    uint32_t copy_h;
    uint32_t stride_px;

    if (drm == NULL || drm->map == NULL || surface == NULL || surface->pixels == NULL) {
        return;
    }
    copy_w = (surface->width < drm->width) ? surface->width : drm->width;
    copy_h = (surface->height < drm->height) ? surface->height : drm->height;
    stride_px = drm->pitch / (uint32_t)sizeof(uint32_t);

    for (uint32_t r = 0U; r < copy_h; r++) {
        memcpy(&drm->map[(size_t)r * (size_t)stride_px], &surface->pixels[(size_t)r * (size_t)surface->width],
               (size_t)copy_w * sizeof(uint32_t));
    }
}

static void drm_close_output(cb_drm_output_t *drm)
{
    if (drm == NULL || drm->fd < 0) {
        return;
    }
    if (drm->map != NULL && drm->map != MAP_FAILED) {
        (void)munmap(drm->map, (size_t)drm->size);
    }
    if (drm->fb_id != 0U) {
        (void)drm_xioctl(drm->fd, DRM_IOCTL_MODE_RMFB, &drm->fb_id);
    }
    if (drm->handle != 0U) {
        struct drm_mode_destroy_dumb destroy = {.handle = drm->handle};
        (void)drm_xioctl(drm->fd, DRM_IOCTL_MODE_DESTROY_DUMB, &destroy);
    }
    (void)close(drm->fd);
    drm->fd = -1;
}
#endif

static int run_headless_once(const char *snapshots_dir, const char *node_name)
{
    cb_renderer_t renderer;
    cb_render_err_t err;

    err = cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, node_name);
    if (err != CB_RENDER_OK) {
        return 1;
    }
    (void)cb_renderer_load_from_store(&renderer, snapshots_dir, 0ULL);
    err = cb_renderer_render_frame(&renderer, 0ULL);
    if (err != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    printf("chalkboard-render %s mode=%d hash=%016llx\n", cb_render_version(), (int)renderer.mode,
           (unsigned long long)cb_surface_hash(&renderer.frame));
    cb_renderer_free(&renderer);
    return 0;
}

static int benchmark_transitions(void)
{
    cb_transition_t transitions[] = {CB_TRANSITION_CUT, CB_TRANSITION_FADE, CB_TRANSITION_SLIDE};
    const char *trans_names[] = {"cut", "fade", "slide"};
    cb_rotation_t rotations[] = {CB_ROTATION_0, CB_ROTATION_90, CB_ROTATION_180, CB_ROTATION_270};
    int rot_angles[] = {0, 90, 180, 270};

    const uint32_t num_frames = 100U;
    const uint32_t duration_ms = 300U;
    int ret = 0;

    printf("=== Chalkboard Renderer Benchmark (1080p, target >= 30 fps) ===\n");

    for (size_t t = 0; t < 3; t++) {
        for (size_t r = 0; r < 4; r++) {
            uint32_t rw = (rotations[r] == CB_ROTATION_90 || rotations[r] == CB_ROTATION_270) ? CB_DEFAULT_HEIGHT
                                                                                              : CB_DEFAULT_WIDTH;
            uint32_t rh = (rotations[r] == CB_ROTATION_90 || rotations[r] == CB_ROTATION_270) ? CB_DEFAULT_WIDTH
                                                                                              : CB_DEFAULT_HEIGHT;
            cb_surface_t s_from;
            cb_surface_t s_to;
            cb_surface_t s_dest;
            struct timespec t_start;
            struct timespec t_end;
            double total_sec;
            double fps;
            double ms_per_frame;
            bool pass;

            if (cb_surface_init(&s_from, rw, rh) != CB_RENDER_OK || cb_surface_init(&s_to, rw, rh) != CB_RENDER_OK ||
                cb_surface_init(&s_dest, rw, rh) != CB_RENDER_OK) {
                (void)fputs("Failed to allocate benchmark surfaces\n", stderr);
                return 1;
            }
            cb_surface_clear(&s_from, 0x0014110fU);
            cb_surface_clear(&s_to, 0x00f2c14eU);

            clock_gettime(CLOCK_MONOTONIC, &t_start);

            for (uint32_t f = 0; f < num_frames; f++) {
                uint32_t elapsed_ms = (f * duration_ms) / num_frames;
                cb_surface_transition(&s_dest, &s_from, &s_to, transitions[t], elapsed_ms, duration_ms);
            }

            clock_gettime(CLOCK_MONOTONIC, &t_end);
            cb_surface_free(&s_from);
            cb_surface_free(&s_to);
            cb_surface_free(&s_dest);

            total_sec = (double)(t_end.tv_sec - t_start.tv_sec) + (((double)(t_end.tv_nsec - t_start.tv_nsec)) / 1e9);
            fps = (double)num_frames / total_sec;
            ms_per_frame = (total_sec * 1000.0) / (double)num_frames;

            pass = (fps >= 30.0);
            printf("BENCHMARK transition=%-5s rotation=%-3d frames=%u total_ms=%.1f ms/frame=%.2f fps=%.1f result=%s\n",
                   trans_names[t], rot_angles[r], num_frames, total_sec * 1000.0, ms_per_frame, fps,
                   pass ? "PASS" : "FAIL");
            if (!pass) {
                ret = 1;
            }
        }
    }
    return ret;
}

static int benchmark_first_frame(void)
{
    cb_renderer_t renderer;
    struct timespec t_start;
    struct timespec t_end;
    double first_frame_ms;
    bool ff_pass;

    clock_gettime(CLOCK_MONOTONIC, &t_start);
    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "bench-node") != CB_RENDER_OK) {
        return 1;
    }
    (void)cb_renderer_render_frame(&renderer, 0ULL);
    clock_gettime(CLOCK_MONOTONIC, &t_end);
    first_frame_ms =
        ((double)(t_end.tv_sec - t_start.tv_sec) + (((double)(t_end.tv_nsec - t_start.tv_nsec)) / 1e9)) * 1000.0;
    ff_pass = (first_frame_ms <= 1000.0);
    printf("BENCHMARK first_frame mode=%d time_ms=%.2f budget_ms=1000.0 result=%s\n", (int)renderer.mode,
           first_frame_ms, ff_pass ? "PASS" : "FAIL");
    cb_renderer_free(&renderer);
    return ff_pass ? 0 : 1;
}

static int run_benchmark(void)
{
    int ret = benchmark_transitions();
    if (benchmark_first_frame() != 0) {
        ret = 1;
    }
    return ret;
}

#ifdef __linux__
static int run_drm(const char *snapshots_dir, const char *node_name, bool daemon_mode)
{
    cb_drm_output_t drm_out;
    int found_card = -1;

    memset(&drm_out, 0, sizeof(drm_out));
    drm_out.fd = -1;

    for (int c = 0; c < 8; c++) {
        char path[32];
        int fd;

        (void)snprintf(path, sizeof(path), "/dev/dri/card%d", c);
        fd = open(path, O_RDWR);
        if (fd < 0) {
            continue;
        }
        if (drm_find_output(fd, &drm_out) == 0) {
            drm_out.fd = fd;
            found_card = c;
            break;
        }
        (void)close(fd);
    }

    if (found_card >= 0 && drm_setup_dumb_fb(&drm_out) == 0) {
        cb_renderer_t renderer;
        printf("DRM output initialized on card%d: %ux%u connector=%u crtc=%u\n", found_card, drm_out.width,
               drm_out.height, drm_out.connector_id, drm_out.crtc_id);

        if (cb_renderer_init(&renderer, drm_out.width, drm_out.height, node_name) == CB_RENDER_OK) {
            (void)cb_renderer_load_from_store(&renderer, snapshots_dir, 0ULL);
            (void)cb_renderer_render_frame(&renderer, 0ULL);
            drm_present_surface(&drm_out, &renderer.frame);
            if (daemon_mode) {
                while (1) {
                    (void)sleep(1);
                }
            }
            cb_renderer_free(&renderer);
        }
        drm_close_output(&drm_out);
    } else if (daemon_mode) {
        /* Headless / disconnected fallback */
        while (1) {
            (void)sleep(1);
        }
    }
    return 0;
}
#endif

int main(int argc, char **argv)
{
    const char *snapshots_dir = "/data/snapshots";
    const char *node_name = "chalkboard-node";
    bool headless_once = false;
    bool try_drm = false;
    bool daemon_mode = false;
    bool run_bench = false;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--version") == 0) {
            printf("chalkboard-render %s\n", cb_render_version());
            return 0;
        }
        if (strcmp(argv[i], "--snapshots") == 0 && (i + 1) < argc) {
            i++;
            snapshots_dir = argv[i];
        } else if (strcmp(argv[i], "--node-name") == 0 && (i + 1) < argc) {
            i++;
            node_name = argv[i];
        } else if (strcmp(argv[i], "--headless-once") == 0) {
            headless_once = true;
        } else if (strcmp(argv[i], "--drm") == 0) {
            try_drm = true;
        } else if (strcmp(argv[i], "--daemon") == 0) {
            daemon_mode = true;
        } else if (strcmp(argv[i], "--benchmark") == 0) {
            run_bench = true;
        }
    }

    if (run_bench) {
        return run_benchmark();
    }

    if (headless_once) {
        return run_headless_once(snapshots_dir, node_name);
    }

#ifdef __linux__
    if (try_drm) {
        return run_drm(snapshots_dir, node_name, daemon_mode);
    }
#else
    (void)try_drm;
    (void)snapshots_dir;
    (void)node_name;
    if (daemon_mode) {
        while (1) {
            (void)sleep(1);
        }
    }
#endif

    printf("chalkboard-render %s\n", cb_render_version());
    return 0;
}
