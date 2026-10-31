/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * chalkboard-render: draws the current snapshot via DRM/KMS (SPEC §7, RND-001).
 */

#ifdef __linux__
#include <drm/drm.h>
#include <drm/drm_mode.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#endif

#include <stdio.h>
#include <string.h>

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

int main(int argc, char **argv)
{
    const char *snapshots_dir = "/data/snapshots";
    const char *node_name = "chalkboard-node";
    bool headless_once = false;
    bool try_drm = false;

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
        }
    }

    if (headless_once) {
        return run_headless_once(snapshots_dir, node_name);
    }

#ifdef __linux__
    if (try_drm) {
        cb_drm_output_t drm_out;
        memset(&drm_out, 0, sizeof(drm_out));
        drm_out.fd = open("/dev/dri/card0", O_RDWR);
        if (drm_out.fd >= 0 && drm_setup_dumb_fb(&drm_out) == 0) {
            cb_renderer_t renderer;
            if (cb_renderer_init(&renderer, drm_out.width, drm_out.height, node_name) == CB_RENDER_OK) {
                (void)cb_renderer_load_from_store(&renderer, snapshots_dir, 0ULL);
                (void)cb_renderer_render_frame(&renderer, 0ULL);
                drm_present_surface(&drm_out, &renderer.frame);
                cb_renderer_free(&renderer);
            }
        }
        drm_close_output(&drm_out);
    }
#else
    (void)try_drm;
#endif

    printf("chalkboard-render %s\n", cb_render_version());
    return 0;
}
