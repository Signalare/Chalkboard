/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include "chalkboard/render.h"

#include <stdlib.h>
#include <string.h>

static uint32_t blend_xrgb(uint32_t bg, uint32_t fg, uint8_t alpha)
{
    uint32_t inv;
    uint32_t r;
    uint32_t g;
    uint32_t b;

    if (alpha == 255U) {
        return fg & 0x00ffffffU;
    }
    if (alpha == 0U) {
        return bg & 0x00ffffffU;
    }
    inv = 255U - (uint32_t)alpha;
    r = ((((fg >> 16U) & 0xffU) * (uint32_t)alpha) + (((bg >> 16U) & 0xffU) * inv)) / 255U;
    g = ((((fg >> 8U) & 0xffU) * (uint32_t)alpha) + (((bg >> 8U) & 0xffU) * inv)) / 255U;
    b = (((fg & 0xffU) * (uint32_t)alpha) + ((bg & 0xffU) * inv)) / 255U;
    return (r << 16U) | (g << 8U) | b;
}

cb_render_err_t cb_surface_init(cb_surface_t *surface, uint32_t width, uint32_t height)
{
    size_t count;

    if (surface == NULL || width == 0U || height == 0U || width > CB_4K_WIDTH || height > CB_4K_HEIGHT) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    count = (size_t)width * (size_t)height;
    surface->pixels = calloc(count, sizeof(uint32_t));
    if (surface->pixels == NULL) {
        surface->width = 0U;
        surface->height = 0U;
        return CB_RENDER_ERR_NOMEM;
    }
    surface->width = width;
    surface->height = height;
    return CB_RENDER_OK;
}

void cb_surface_free(cb_surface_t *surface)
{
    if (surface == NULL) {
        return;
    }
    free(surface->pixels);
    surface->pixels = NULL;
    surface->width = 0U;
    surface->height = 0U;
}

void cb_surface_clear(cb_surface_t *surface, uint32_t xrgb)
{
    size_t total;
    uint32_t color;

    if (surface == NULL || surface->pixels == NULL) {
        return;
    }
    color = xrgb & 0x00ffffffU;
    total = (size_t)surface->width * (size_t)surface->height;
    for (size_t i = 0; i < total; i++) {
        surface->pixels[i] = color;
    }
}

void cb_surface_fill_rect(cb_surface_t *surface, uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint32_t xrgb,
                          uint8_t alpha)
{
    uint32_t x_end;
    uint32_t y_end;

    if (surface == NULL || surface->pixels == NULL || alpha == 0U || w == 0U || h == 0U) {
        return;
    }
    if (x >= surface->width || y >= surface->height) {
        return;
    }
    x_end = x + w;
    if (x_end > surface->width || x_end < x) {
        x_end = surface->width;
    }
    y_end = y + h;
    if (y_end > surface->height || y_end < y) {
        y_end = surface->height;
    }

    for (uint32_t row = y; row < y_end; row++) {
        uint32_t *line = &surface->pixels[(size_t)row * (size_t)surface->width];
        for (uint32_t col = x; col < x_end; col++) {
            line[col] = blend_xrgb(line[col], xrgb, alpha);
        }
    }
}

uint64_t cb_surface_hash(const cb_surface_t *surface)
{
    uint64_t hash = 0xcbf29ce484222325ULL;
    size_t total;

    if (surface == NULL || surface->pixels == NULL) {
        return 0ULL;
    }
    total = (size_t)surface->width * (size_t)surface->height;
    for (size_t i = 0; i < total; i++) {
        uint32_t px = surface->pixels[i];
        hash ^= (uint64_t)(px & 0xffU);
        hash *= 0x100000001b3ULL;
        hash ^= (uint64_t)((px >> 8U) & 0xffU);
        hash *= 0x100000001b3ULL;
        hash ^= (uint64_t)((px >> 16U) & 0xffU);
        hash *= 0x100000001b3ULL;
    }
    return hash;
}

void cb_map_rotated_rect(cb_rotation_t rotation, uint32_t canvas_w, uint32_t canvas_h, uint32_t x, uint32_t y,
                         uint32_t w, uint32_t h, uint32_t *out_x, uint32_t *out_y, uint32_t *out_w, uint32_t *out_h)
{
    uint32_t clamped_x = x < canvas_w ? x : canvas_w;
    uint32_t clamped_y = y < canvas_h ? y : canvas_h;
    uint32_t clamped_w = (clamped_x + w <= canvas_w) ? w : (canvas_w - clamped_x);
    uint32_t clamped_h = (clamped_y + h <= canvas_h) ? h : (canvas_h - clamped_y);

    if (out_x == NULL || out_y == NULL || out_w == NULL || out_h == NULL) {
        return;
    }

    switch (rotation) {
    case CB_ROTATION_90:
        *out_x = canvas_h - (clamped_y + clamped_h);
        *out_y = clamped_x;
        *out_w = clamped_h;
        *out_h = clamped_w;
        break;
    case CB_ROTATION_180:
        *out_x = canvas_w - (clamped_x + clamped_w);
        *out_y = canvas_h - (clamped_y + clamped_h);
        *out_w = clamped_w;
        *out_h = clamped_h;
        break;
    case CB_ROTATION_270:
        *out_x = clamped_y;
        *out_y = canvas_w - (clamped_x + clamped_w);
        *out_w = clamped_h;
        *out_h = clamped_w;
        break;
    case CB_ROTATION_0:
    default:
        *out_x = clamped_x;
        *out_y = clamped_y;
        *out_w = clamped_w;
        *out_h = clamped_h;
        break;
    }
}

static void copy_surface_pixels(cb_surface_t *dest, const cb_surface_t *src)
{
    size_t bytes = (size_t)dest->width * (size_t)dest->height * sizeof(uint32_t);
    memcpy(dest->pixels, src->pixels, bytes);
}

static void transition_fade(cb_surface_t *dest, const cb_surface_t *from, const cb_surface_t *to, uint8_t alpha)
{
    size_t total = (size_t)dest->width * (size_t)dest->height;
    for (size_t i = 0; i < total; i++) {
        dest->pixels[i] = blend_xrgb(from->pixels[i], to->pixels[i], alpha);
    }
}

static void transition_slide(cb_surface_t *dest, const cb_surface_t *from, const cb_surface_t *to, uint32_t offset_x)
{
    uint32_t width = dest->width;
    uint32_t height = dest->height;
    uint32_t split = (offset_x < width) ? (width - offset_x) : 0U;

    for (uint32_t row = 0; row < height; row++) {
        uint32_t *dst_row = &dest->pixels[(size_t)row * (size_t)width];
        const uint32_t *from_row = &from->pixels[(size_t)row * (size_t)width];
        const uint32_t *to_row = &to->pixels[(size_t)row * (size_t)width];

        for (uint32_t col = 0; col < split; col++) {
            dst_row[col] = from_row[col + offset_x];
        }
        for (uint32_t col = split; col < width; col++) {
            dst_row[col] = to_row[col - split];
        }
    }
}

void cb_surface_transition(cb_surface_t *dest, const cb_surface_t *from, const cb_surface_t *to,
                           cb_transition_t transition, uint32_t elapsed_ms, uint32_t duration_ms)
{
    if (dest == NULL || from == NULL || to == NULL || dest->pixels == NULL || from->pixels == NULL ||
        to->pixels == NULL) {
        return;
    }
    if (dest->width != from->width || dest->height != from->height || dest->width != to->width ||
        dest->height != to->height) {
        return;
    }
    if (transition == CB_TRANSITION_CUT || duration_ms == 0U || elapsed_ms >= duration_ms) {
        copy_surface_pixels(dest, to);
        return;
    }
    if (elapsed_ms == 0U) {
        copy_surface_pixels(dest, from);
        return;
    }
    if (transition == CB_TRANSITION_FADE) {
        uint8_t alpha = (uint8_t)((elapsed_ms * 255U) / duration_ms);
        transition_fade(dest, from, to, alpha);
        return;
    }
    if (transition == CB_TRANSITION_SLIDE) {
        uint32_t offset_x = (uint32_t)(((uint64_t)dest->width * (uint64_t)elapsed_ms) / (uint64_t)duration_ms);
        transition_slide(dest, from, to, offset_x);
        return;
    }
    copy_surface_pixels(dest, to);
}
