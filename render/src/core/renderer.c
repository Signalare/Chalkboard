/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include "internal.h"

#include <stdio.h>
#include <string.h>

static void clear_cached_images(cb_renderer_t *renderer)
{
    for (uint32_t i = 0U; i < renderer->cached_image_count; i++) {
        cb_cached_image_free(&renderer->cached_images[i]);
    }
    renderer->cached_image_count = 0U;
}

static const cb_cached_image_t *find_cached_image(const cb_renderer_t *renderer, const char *image_hash)
{
    if (image_hash == NULL || image_hash[0] == '\0') {
        return NULL;
    }
    for (uint32_t i = 0U; i < renderer->cached_image_count; i++) {
        if (strcmp(renderer->cached_images[i].sha256_hex, image_hash) == 0) {
            return &renderer->cached_images[i];
        }
    }
    return NULL;
}

static void preload_snapshot_images(cb_renderer_t *renderer, const char *snapshots_root, const char *snap_hash)
{
    clear_cached_images(renderer);
    for (uint32_t p = 0U; p < renderer->snapshot.page_count; p++) {
        const cb_snapshot_page_t *page = &renderer->snapshot.pages[p];
        for (uint32_t i = 0U; i < page->item_count; i++) {
            const char *img_hash = page->items[i].image_hash;
            if (img_hash[0] == '\0' || find_cached_image(renderer, img_hash) != NULL) {
                continue;
            }
            if (renderer->cached_image_count >= CB_MAX_CACHED_IMAGES) {
                return;
            }
            if (cb_render_cache_load_image(snapshots_root, snap_hash, img_hash,
                                           &renderer->cached_images[renderer->cached_image_count]) == CB_RENDER_OK) {
                renderer->cached_image_count++;
            }
        }
    }
}

static bool page_is_scheduled(const cb_snapshot_page_t *page, uint16_t minute_of_day)
{
    if (page->schedule_start_min == 0U && page->schedule_end_min == 0U) {
        return true;
    }
    if (page->schedule_start_min <= page->schedule_end_min) {
        if (minute_of_day >= page->schedule_start_min && minute_of_day < page->schedule_end_min) {
            return true;
        }
        return false;
    }
    if (minute_of_day >= page->schedule_start_min || minute_of_day < page->schedule_end_min) {
        return true;
    }
    return false;
}

static uint32_t count_visible_items(const cb_snapshot_page_t *page, cb_sold_out_mode_t mode)
{
    uint32_t count = 0U;

    for (uint32_t i = 0U; i < page->item_count; i++) {
        if (mode == CB_SOLD_OUT_HIDE && page->items[i].sold_out) {
            continue;
        }
        count++;
    }
    return count;
}

static void append_page_subpages(cb_renderer_t *renderer, uint32_t page_idx, const cb_snapshot_page_t *page)
{
    uint32_t visible = count_visible_items(page, renderer->snapshot.sold_out_mode);
    uint32_t cap = (page->repeater_capacity == 0U) ? 8U : page->repeater_capacity;
    uint32_t sub_count = (visible == 0U) ? 1U : ((visible + cap - 1U) / cap);
    uint32_t dur = (page->duration_ms == 0U) ? 8000U : page->duration_ms;

    for (uint32_t s = 0U; s < sub_count && renderer->subpage_total < CB_MAX_SUBPAGES; s++) {
        cb_subpage_slot_t *slot = &renderer->subpages[renderer->subpage_total];
        uint32_t start = s * cap;
        uint32_t rem = (visible > start) ? (visible - start) : 0U;

        slot->page_index = page_idx;
        slot->subpage_index = s;
        slot->subpage_count = sub_count;
        slot->item_start = start;
        slot->item_count = (rem > cap) ? cap : rem;
        slot->duration_ms = dur;
        slot->transition = page->transition;
        slot->transition_ms = page->transition_ms;
        renderer->subpage_total++;
    }
}

static void rebuild_subpages(cb_renderer_t *renderer, uint16_t local_minute_of_day)
{
    renderer->current_minute_of_day = local_minute_of_day;
    renderer->subpage_total = 0U;
    if (!renderer->has_snapshot) {
        return;
    }
    for (uint32_t p = 0U; p < renderer->snapshot.page_count; p++) {
        const cb_snapshot_page_t *page = &renderer->snapshot.pages[p];
        if (!page_is_scheduled(page, local_minute_of_day)) {
            continue;
        }
        append_page_subpages(renderer, p, page);
    }
    if (renderer->subpage_total == 0U && renderer->snapshot.page_count > 0U) {
        append_page_subpages(renderer, 0U, &renderer->snapshot.pages[0]);
    }
    if (renderer->active_subpage >= renderer->subpage_total) {
        renderer->active_subpage = 0U;
    }
}

cb_render_err_t cb_renderer_init(cb_renderer_t *renderer, uint32_t output_width, uint32_t output_height,
                                 const char *node_name)
{
    cb_render_err_t err;

    if (renderer == NULL || output_width == 0U || output_height == 0U) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    memset(renderer, 0, sizeof(*renderer));
    renderer->output_width = output_width;
    renderer->output_height = output_height;
    renderer->rotation = CB_ROTATION_0;
    renderer->mode = CB_SCREEN_MODE_STARTING;
    renderer->current_minute_of_day = 720U;
    (void)snprintf(renderer->node_name, sizeof(renderer->node_name), "%s",
                   (node_name != NULL && node_name[0] != '\0') ? node_name : "chalkboard-node");

    err = cb_surface_init(&renderer->frame, output_width, output_height);
    if (err != CB_RENDER_OK) {
        return err;
    }
    err = cb_surface_init(&renderer->prev_frame, output_width, output_height);
    if (err != CB_RENDER_OK) {
        cb_surface_free(&renderer->frame);
        return err;
    }
    err = cb_surface_init(&renderer->next_frame, output_width, output_height);
    if (err != CB_RENDER_OK) {
        cb_surface_free(&renderer->prev_frame);
        cb_surface_free(&renderer->frame);
        return err;
    }
    return CB_RENDER_OK;
}

void cb_renderer_free(cb_renderer_t *renderer)
{
    if (renderer == NULL) {
        return;
    }
    clear_cached_images(renderer);
    cb_surface_free(&renderer->next_frame);
    cb_surface_free(&renderer->prev_frame);
    cb_surface_free(&renderer->frame);
}

void cb_renderer_set_setup_info(cb_renderer_t *renderer, bool first_boot_setup, const char *ssid, const char *url)
{
    if (renderer == NULL) {
        return;
    }
    renderer->first_boot_setup = first_boot_setup;
    (void)snprintf(renderer->setup_ssid, sizeof(renderer->setup_ssid), "%s", ssid != NULL ? ssid : "");
    (void)snprintf(renderer->setup_url, sizeof(renderer->setup_url), "%s", url != NULL ? url : "");
    if (!renderer->has_snapshot) {
        renderer->mode = first_boot_setup ? CB_SCREEN_MODE_SETUP : CB_SCREEN_MODE_STARTING;
    }
}

void cb_renderer_set_custom_fallback_font(cb_renderer_t *renderer, bool enabled)
{
    if (renderer != NULL) {
        renderer->custom_fallback_loaded = enabled;
    }
}

static cb_render_err_t activate_loaded_snapshot(cb_renderer_t *renderer, const char *snapshots_root,
                                                const cb_snapshot_t *loaded, const char *hash, bool record_previous,
                                                uint64_t now_ms)
{
    if (record_previous && renderer->active_hash[0] != '\0' && strcmp(renderer->active_hash, hash) != 0) {
        (void)snprintf(renderer->fallback_hash, sizeof(renderer->fallback_hash), "%s", renderer->active_hash);
        (void)cb_snapshot_set_pointer_atomic(snapshots_root, "previous", renderer->active_hash);
    }
    renderer->snapshot = *loaded;
    renderer->rotation = loaded->rotation;
    renderer->has_snapshot = true;
    renderer->mode = CB_SCREEN_MODE_CONTENT;
    (void)snprintf(renderer->active_hash, sizeof(renderer->active_hash), "%s", hash);
    preload_snapshot_images(renderer, snapshots_root, hash);
    renderer->active_subpage = 0U;
    renderer->previous_subpage = 0U;
    renderer->in_transition = false;
    renderer->page_entered_ms = now_ms;
    rebuild_subpages(renderer, renderer->current_minute_of_day);
    return CB_RENDER_OK;
}

cb_render_err_t cb_renderer_load_from_store(cb_renderer_t *renderer, const char *snapshots_root, uint64_t now_ms)
{
    char cur_hash[CB_MAX_HASH_HEX];
    char prev_hash[CB_MAX_HASH_HEX];
    cb_snapshot_t candidate;

    if (renderer == NULL || snapshots_root == NULL) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (cb_snapshot_read_pointer(snapshots_root, "current", cur_hash, sizeof(cur_hash)) == CB_RENDER_OK &&
        cb_snapshot_load_by_hash(snapshots_root, cur_hash, &candidate) == CB_RENDER_OK) {
        if (cb_snapshot_read_pointer(snapshots_root, "previous", prev_hash, sizeof(prev_hash)) == CB_RENDER_OK) {
            (void)snprintf(renderer->fallback_hash, sizeof(renderer->fallback_hash), "%s", prev_hash);
        }
        return activate_loaded_snapshot(renderer, snapshots_root, &candidate, cur_hash, true, now_ms);
    }

    /* Current snapshot missing or invalid: fall back to `previous` (STO-004). */
    if (cb_snapshot_read_pointer(snapshots_root, "previous", prev_hash, sizeof(prev_hash)) == CB_RENDER_OK &&
        cb_snapshot_load_by_hash(snapshots_root, prev_hash, &candidate) == CB_RENDER_OK) {
        return activate_loaded_snapshot(renderer, snapshots_root, &candidate, prev_hash, false, now_ms);
    }

    /* If we already have an in-memory good snapshot, keep rendering it (STO-004, RND-007). */
    if (renderer->has_snapshot) {
        return CB_RENDER_OK;
    }

    renderer->mode = renderer->first_boot_setup ? CB_SCREEN_MODE_SETUP : CB_SCREEN_MODE_STARTING;
    return CB_RENDER_ERR_NO_SNAPSHOT;
}

cb_render_err_t cb_renderer_set_sold_out(cb_renderer_t *renderer, const char *item_id, bool sold_out)
{
    bool found = false;

    if (renderer == NULL || item_id == NULL || item_id[0] == '\0') {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (!renderer->has_snapshot) {
        return CB_RENDER_ERR_NO_SNAPSHOT;
    }
    for (uint32_t p = 0U; p < renderer->snapshot.page_count; p++) {
        cb_snapshot_page_t *page = &renderer->snapshot.pages[p];
        for (uint32_t i = 0U; i < page->item_count; i++) {
            if (strcmp(page->items[i].id, item_id) == 0) {
                page->items[i].sold_out = sold_out;
                found = true;
            }
        }
    }
    if (!found) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    rebuild_subpages(renderer, renderer->current_minute_of_day);
    return CB_RENDER_OK;
}

void cb_renderer_set_announcement(cb_renderer_t *renderer, const char *text, bool active)
{
    if (renderer == NULL) {
        return;
    }
    if (active && (text != NULL) && (text[0] != '\0')) {
        renderer->announcement_active = true;
    } else {
        renderer->announcement_active = false;
    }
    (void)snprintf(renderer->announcement_text, sizeof(renderer->announcement_text), "%s", text != NULL ? text : "");
}

void cb_renderer_set_identify(cb_renderer_t *renderer, bool enabled, const char *node_name, const char *screen_name,
                              const char *ip_address)
{
    if (renderer == NULL) {
        return;
    }
    renderer->identify.enabled = enabled;
    (void)snprintf(renderer->identify.node_name, sizeof(renderer->identify.node_name), "%s",
                   node_name != NULL ? node_name : renderer->node_name);
    (void)snprintf(renderer->identify.screen_name, sizeof(renderer->identify.screen_name), "%s",
                   screen_name != NULL ? screen_name : "HDMI-A-1");
    (void)snprintf(renderer->identify.ip_address, sizeof(renderer->identify.ip_address), "%s",
                   ip_address != NULL ? ip_address : "0.0.0.0");
}

static void rotate_surface_180(cb_surface_t *surface)
{
    size_t total = (size_t)surface->width * (size_t)surface->height;

    for (size_t i = 0U; i < total / 2U; i++) {
        uint32_t tmp = surface->pixels[i];
        surface->pixels[i] = surface->pixels[total - 1U - i];
        surface->pixels[total - 1U - i] = tmp;
    }
}

static void rotate_surface_quarter(cb_surface_t *surface, cb_rotation_t rotation, cb_surface_t *scratch)
{
    uint32_t w = surface->width;
    uint32_t h = surface->height;
    size_t total = (size_t)w * (size_t)h;

    memcpy(scratch->pixels, surface->pixels, total * sizeof(uint32_t));
    for (uint32_t row = 0U; row < h; row++) {
        uint32_t src_y = (row * w) / h;
        for (uint32_t col = 0U; col < w; col++) {
            uint32_t src_x = (col * h) / w;
            uint32_t rx = (rotation == CB_ROTATION_90) ? src_y : (w - 1U - src_y);
            uint32_t ry = (rotation == CB_ROTATION_90) ? (h - 1U - src_x) : src_x;
            surface->pixels[((size_t)row * (size_t)w) + (size_t)col] =
                scratch->pixels[((size_t)ry * (size_t)w) + (size_t)rx];
        }
    }
}

/* Rotates an entire rendered surface in-place for 90, 180, or 270 degree orientation (RND-002). */
static void apply_surface_rotation(cb_surface_t *surface, cb_rotation_t rotation, cb_surface_t *scratch)
{
    if (surface == NULL || surface->pixels == NULL || rotation == CB_ROTATION_0) {
        return;
    }
    if (rotation == CB_ROTATION_180) {
        rotate_surface_180(surface);
        return;
    }
    if (scratch == NULL || scratch->pixels == NULL || scratch->width != surface->width ||
        scratch->height != surface->height) {
        return;
    }
    rotate_surface_quarter(surface, rotation, scratch);
}

static void blit_cached_image(cb_surface_t *surface, const cb_cached_image_t *img, uint32_t dst_x, uint32_t dst_y,
                              uint32_t dst_w, uint32_t dst_h, uint8_t alpha)
{
    if (img == NULL || img->pixels == NULL || dst_w == 0U || dst_h == 0U) {
        return;
    }
    for (uint32_t r = 0U; r < dst_h; r++) {
        uint32_t src_r = (r * img->height) / dst_h;
        for (uint32_t c = 0U; c < dst_w; c++) {
            uint32_t src_c = (c * img->width) / dst_w;
            uint32_t px = img->pixels[((size_t)src_r * (size_t)img->width) + (size_t)src_c];
            cb_surface_fill_rect(surface, dst_x + c, dst_y + r, 1U, 1U, px, alpha);
        }
    }
}

static const cb_snapshot_item_t *nth_visible_item(const cb_snapshot_page_t *page, cb_sold_out_mode_t mode,
                                                  uint32_t visible_idx)
{
    uint32_t seen = 0U;

    for (uint32_t i = 0U; i < page->item_count; i++) {
        if (mode == CB_SOLD_OUT_HIDE && page->items[i].sold_out) {
            continue;
        }
        if (seen == visible_idx) {
            return &page->items[i];
        }
        seen++;
    }
    return NULL;
}

static void render_single_item_row(cb_renderer_t *renderer, cb_surface_t *target, const cb_snapshot_page_t *page,
                                   const cb_snapshot_item_t *item, uint32_t row_y)
{
    uint8_t alpha = 255U;
    const cb_cached_image_t *img;
    uint32_t text_x = 96U;
    uint32_t row_w = (target->width > 144U) ? (target->width - 144U) : target->width;
    uint32_t price_x = (target->width > 320U) ? (target->width - 320U) : 16U;
    uint32_t badge_x = (target->width > 520U) ? (target->width - 520U) : 16U;

    if (item->sold_out && renderer->snapshot.sold_out_mode == CB_SOLD_OUT_DIM) {
        alpha = (renderer->snapshot.sold_out_dim_alpha > 0U) ? renderer->snapshot.sold_out_dim_alpha : 102U;
    }
    cb_surface_fill_rect(target, 72U, row_y, row_w, 72U, page->accent_color, (uint8_t)(alpha / 4U));

    img = find_cached_image(renderer, item->image_hash);
    if (img != NULL) {
        blit_cached_image(target, img, 84U, row_y + 8U, 56U, 56U, alpha);
        text_x = 156U;
    }

    renderer->fallback_glyph_count +=
        cb_draw_text_utf8(target, text_x, row_y + 14U, item->name, page->body_font, 3U, page->text_color, alpha, false,
                          renderer->custom_fallback_loaded);
    if (item->description[0] != '\0') {
        renderer->fallback_glyph_count +=
            cb_draw_text_utf8(target, text_x, row_y + 42U, item->description, page->body_font, 2U, page->text_color,
                              alpha, false, renderer->custom_fallback_loaded);
    }
    renderer->fallback_glyph_count +=
        cb_draw_text_utf8(target, price_x, row_y + 18U, item->price_primary, CB_FONT_INTER, 3U, page->heading_color,
                          alpha, true, renderer->custom_fallback_loaded);

    if (item->sold_out && renderer->snapshot.sold_out_mode == CB_SOLD_OUT_BADGE) {
        const char *badge =
            (renderer->snapshot.sold_out_badge[0] != '\0') ? renderer->snapshot.sold_out_badge : "SOLD OUT";
        cb_surface_fill_rect(target, badge_x, row_y + 12U, 160U, 44U, page->heading_color, 255U);
        renderer->fallback_glyph_count +=
            cb_draw_text_utf8(target, badge_x + 16U, row_y + 22U, badge, CB_FONT_INTER, 2U, page->bg_color, 255U, false,
                              renderer->custom_fallback_loaded);
    }
}

static void render_subpage_to_surface(cb_renderer_t *renderer, cb_surface_t *target, uint32_t subpage_idx)
{
    const cb_subpage_slot_t *slot;
    const cb_snapshot_page_t *page;

    if (subpage_idx >= renderer->subpage_total) {
        cb_surface_clear(target, 0x0014110fU);
        return;
    }
    slot = &renderer->subpages[subpage_idx];
    page = &renderer->snapshot.pages[slot->page_index];

    cb_surface_clear(target, page->bg_color);
    cb_surface_fill_rect(target, 48U, 36U, target->width - 96U, 84U, page->accent_color, 255U);
    renderer->fallback_glyph_count +=
        cb_draw_text_utf8(target, 72U, 56U, page->title, page->heading_font, 4U, page->heading_color, 255U, false,
                          renderer->custom_fallback_loaded);

    for (uint32_t i = 0U; i < slot->item_count; i++) {
        const cb_snapshot_item_t *item = nth_visible_item(page, renderer->snapshot.sold_out_mode, slot->item_start + i);
        if (item != NULL) {
            render_single_item_row(renderer, target, page, item, 148U + (i * 92U));
        }
    }
}

static void render_starting_or_setup_screen(cb_renderer_t *renderer)
{
    if (renderer->mode == CB_SCREEN_MODE_SETUP) {
        cb_surface_clear(&renderer->frame, 0x00122438U);
        cb_surface_fill_rect(&renderer->frame, 80U, 80U, renderer->frame.width - 160U, 240U, 0x001e3a5fU, 255U);
        (void)cb_draw_text_utf8(&renderer->frame, 120U, 120U, "CHALKBOARD SETUP", CB_FONT_OSWALD, 4U, 0x00f2c14eU, 255U,
                                false, renderer->custom_fallback_loaded);
        (void)cb_draw_text_utf8(&renderer->frame, 120U, 180U, renderer->node_name, CB_FONT_INTER, 3U, 0x00ffffffU, 255U,
                                false, renderer->custom_fallback_loaded);
        (void)cb_draw_text_utf8(&renderer->frame, 120U, 230U, renderer->setup_ssid, CB_FONT_INTER, 2U, 0x00d0e2f2U,
                                255U, false, renderer->custom_fallback_loaded);
        return;
    }

    /* Neutral "Starting…" screen with node name (BOOT-006). */
    cb_surface_clear(&renderer->frame, 0x0018181bU);
    cb_surface_fill_rect(&renderer->frame, 120U, 160U, renderer->frame.width - 240U, 180U, 0x0027272aU, 255U);
    renderer->fallback_glyph_count +=
        cb_draw_text_utf8(&renderer->frame, 160U, 200U, "Starting\xe2\x80\xa6", CB_FONT_INTER, 4U, 0x00f4f4f5U, 255U,
                          false, renderer->custom_fallback_loaded);
    renderer->fallback_glyph_count +=
        cb_draw_text_utf8(&renderer->frame, 160U, 260U, renderer->node_name, CB_FONT_INTER, 3U, 0x00a1a1aaU, 255U,
                          false, renderer->custom_fallback_loaded);
}

static void render_overlays(cb_renderer_t *renderer)
{
    if (renderer->announcement_active) {
        uint32_t bar_h = 72U;
        uint32_t bar_y = (renderer->frame.height > bar_h) ? (renderer->frame.height - bar_h) : 0U;
        cb_surface_fill_rect(&renderer->frame, 0U, bar_y, renderer->frame.width, bar_h, 0x00b91c1cU, 240U);
        renderer->fallback_glyph_count +=
            cb_draw_text_utf8(&renderer->frame, 48U, bar_y + 22U, renderer->announcement_text, CB_FONT_INTER, 3U,
                              0x00ffffffU, 255U, false, renderer->custom_fallback_loaded);
    }

    if (renderer->identify.enabled) {
        cb_surface_fill_rect(&renderer->frame, 60U, 60U, 640U, 200U, 0x00090d16U, 235U);
        cb_surface_fill_rect(&renderer->frame, 60U, 60U, 640U, 8U, 0x0038bdf8U, 255U);
        (void)cb_draw_text_utf8(&renderer->frame, 88U, 92U, renderer->identify.node_name, CB_FONT_OSWALD, 3U,
                                0x0038bdf8U, 255U, false, renderer->custom_fallback_loaded);
        (void)cb_draw_text_utf8(&renderer->frame, 88U, 140U, renderer->identify.screen_name, CB_FONT_INTER, 3U,
                                0x00ffffffU, 255U, false, renderer->custom_fallback_loaded);
        (void)cb_draw_text_utf8(&renderer->frame, 88U, 188U, renderer->identify.ip_address, CB_FONT_INTER, 3U,
                                0x00e2e8f0U, 255U, false, renderer->custom_fallback_loaded);
    }
}

cb_render_err_t cb_renderer_tick(cb_renderer_t *renderer, uint64_t now_ms, uint16_t local_minute_of_day)
{
    const cb_subpage_slot_t *cur_slot;
    uint64_t elapsed;

    if (renderer == NULL) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    if (!renderer->has_snapshot) {
        renderer->current_minute_of_day = local_minute_of_day;
        return CB_RENDER_OK;
    }
    rebuild_subpages(renderer, local_minute_of_day);
    if (renderer->subpage_total <= 1U) {
        renderer->in_transition = false;
        return CB_RENDER_OK;
    }

    cur_slot = &renderer->subpages[renderer->active_subpage];
    elapsed = (now_ms >= renderer->page_entered_ms) ? (now_ms - renderer->page_entered_ms) : 0ULL;
    if (elapsed >= (uint64_t)cur_slot->duration_ms) {
        renderer->previous_subpage = renderer->active_subpage;
        renderer->active_subpage = (renderer->active_subpage + 1U) % renderer->subpage_total;
        renderer->page_entered_ms = now_ms;
        if (cur_slot->transition != CB_TRANSITION_CUT && cur_slot->transition_ms > 0U) {
            renderer->in_transition = true;
            renderer->transition_started_ms = now_ms;
        } else {
            renderer->in_transition = false;
        }
    }
    return CB_RENDER_OK;
}

cb_render_err_t cb_renderer_render_frame(cb_renderer_t *renderer, uint64_t now_ms)
{
    if (renderer == NULL) {
        return CB_RENDER_ERR_INVALID_ARG;
    }
    renderer->fallback_glyph_count = 0U;

    if (!renderer->has_snapshot || renderer->mode != CB_SCREEN_MODE_CONTENT) {
        render_starting_or_setup_screen(renderer);
        render_overlays(renderer);
        apply_surface_rotation(&renderer->frame, renderer->rotation, &renderer->prev_frame);
        return CB_RENDER_OK;
    }

    if (renderer->in_transition) {
        const cb_subpage_slot_t *slot = &renderer->subpages[renderer->active_subpage];
        uint64_t elapsed =
            (now_ms >= renderer->transition_started_ms) ? (now_ms - renderer->transition_started_ms) : 0U;
        if (elapsed >= (uint64_t)slot->transition_ms) {
            renderer->in_transition = false;
            render_subpage_to_surface(renderer, &renderer->frame, renderer->active_subpage);
        } else {
            render_subpage_to_surface(renderer, &renderer->prev_frame, renderer->previous_subpage);
            render_subpage_to_surface(renderer, &renderer->next_frame, renderer->active_subpage);
            cb_surface_transition(&renderer->frame, &renderer->prev_frame, &renderer->next_frame, slot->transition,
                                  (uint32_t)elapsed, slot->transition_ms);
        }
    } else {
        render_subpage_to_surface(renderer, &renderer->frame, renderer->active_subpage);
    }

    render_overlays(renderer);
    apply_surface_rotation(&renderer->frame, renderer->rotation, &renderer->prev_frame);
    return CB_RENDER_OK;
}
