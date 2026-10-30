/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "chalkboard/render.h"

static void make_temp_dir(char *buf, size_t size, const char *label)
{
    static unsigned int seq = 0U;
    seq++;
    (void)snprintf(buf, size, "/tmp/chalkboard-render-test-%ld-%s-%u", (long)getpid(), label, seq);
}

static void populate_sample_snapshot(cb_snapshot_t *snap, const char *hash, uint64_t pub_id, uint32_t item_count)
{
    memset(snap, 0, sizeof(*snap));
    snap->schema_version = 1U;
    snap->publication_id = pub_id;
    (void)snprintf(snap->sha256_hex, sizeof(snap->sha256_hex), "%s", hash);
    snap->logical_width = CB_DEFAULT_WIDTH;
    snap->logical_height = CB_DEFAULT_HEIGHT;
    snap->rotation = CB_ROTATION_0;
    snap->sold_out_mode = CB_SOLD_OUT_BADGE;
    (void)snprintf(snap->sold_out_badge, sizeof(snap->sold_out_badge), "SLUT");
    snap->sold_out_dim_alpha = 96U;
    snap->page_count = 1U;

    snap->pages[0].heading_font = CB_FONT_OSWALD;
    snap->pages[0].body_font = CB_FONT_INTER;
    snap->pages[0].bg_color = 0x0014110fU;
    snap->pages[0].heading_color = 0x00f2c14eU;
    snap->pages[0].text_color = 0x00f5f1eaU;
    snap->pages[0].accent_color = 0x0026211dU;
    snap->pages[0].duration_ms = 5000U;
    snap->pages[0].transition = CB_TRANSITION_FADE;
    snap->pages[0].transition_ms = 300U;
    snap->pages[0].repeater_capacity = 4U;
    (void)snprintf(snap->pages[0].id, sizeof(snap->pages[0].id), "page-main");
    (void)snprintf(snap->pages[0].title, sizeof(snap->pages[0].title), "WARM DRINKS");
    snap->pages[0].item_count = item_count;

    for (uint32_t i = 0U; i < item_count; i++) {
        cb_snapshot_item_t *item = &snap->pages[0].items[i];
        uint32_t price_kr = 35U + (i * 5U);
        (void)snprintf(item->id, sizeof(item->id), "item-%u", i);
        (void)snprintf(item->name, sizeof(item->name), "Espresso #%u", i);
        (void)snprintf(item->description, sizeof(item->description), "Double shot organic roast");
        (void)snprintf(item->price_primary, sizeof(item->price_primary), "%u kr", price_kr);
        item->sold_out = false;
    }
}

/* BOOT-005, BOOT-006: Neutral "Starting..." and first-boot setup screens when no snapshot exists. */
static int test_starting_and_setup_screens(void)
{
    char store_dir[CB_MAX_PATH_LEN];
    cb_renderer_t renderer;
    uint64_t starting_hash;
    uint64_t setup_hash;

    make_temp_dir(store_dir, sizeof(store_dir), "boot-screens");
    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "pi4-kitchen") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 0ULL) != CB_RENDER_ERR_NO_SNAPSHOT) {
        cb_renderer_free(&renderer);
        return 1;
    }
    if (renderer.mode != CB_SCREEN_MODE_STARTING) {
        cb_renderer_free(&renderer);
        return 1;
    }
    if (cb_renderer_render_frame(&renderer, 0ULL) != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    /* U+2026 ellipsis in "Starting…" triggers automatic Noto Sans glyph fallback (RND-014). */
    if (renderer.fallback_glyph_count == 0U) {
        cb_renderer_free(&renderer);
        return 1;
    }
    starting_hash = cb_surface_hash(&renderer.frame);

    cb_renderer_set_setup_info(&renderer, true, "chalkboard-setup-a1b2", "http://192.168.4.1");
    if (renderer.mode != CB_SCREEN_MODE_SETUP) {
        cb_renderer_free(&renderer);
        return 1;
    }
    if (cb_renderer_render_frame(&renderer, 0ULL) != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    setup_hash = cb_surface_hash(&renderer.frame);
    cb_renderer_free(&renderer);

    return (starting_hash != 0ULL && setup_hash != 0ULL && starting_hash != setup_hash) ? 0 : 1;
}

/* STO-002, STO-003, STO-004: Atomic snapshot write, `current` pointer, and fallback to `previous` on corruption. */
static int test_atomic_snapshot_and_fallback(void)
{
    char store_dir[CB_MAX_PATH_LEN];
    char bad_snap_dir[CB_MAX_PATH_LEN];
    cb_snapshot_t snap_v1;
    cb_snapshot_t snap_v2;
    cb_renderer_t renderer;
    const uint8_t corrupt_bytes[16] = {0xdeU, 0xadU, 0xbeU, 0xefU};

    make_temp_dir(store_dir, sizeof(store_dir), "atomic-fallback");
    populate_sample_snapshot(&snap_v1, "11111111111111111111111111111111", 1ULL, 3U);
    populate_sample_snapshot(&snap_v2, "22222222222222222222222222222222", 2ULL, 4U);

    if (cb_snapshot_save_to_store(store_dir, &snap_v1) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", snap_v1.sha256_hex) != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "pi4-counter") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 100ULL) != CB_RENDER_OK ||
        strcmp(renderer.active_hash, snap_v1.sha256_hex) != 0) {
        cb_renderer_free(&renderer);
        return 1;
    }

    /* Publish v2; v1 must be retained as `previous` (STO-004). */
    if (cb_snapshot_save_to_store(store_dir, &snap_v2) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", snap_v2.sha256_hex) != CB_RENDER_OK ||
        cb_renderer_load_from_store(&renderer, store_dir, 200ULL) != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    if (strcmp(renderer.active_hash, snap_v2.sha256_hex) != 0 ||
        strcmp(renderer.fallback_hash, snap_v1.sha256_hex) != 0) {
        cb_renderer_free(&renderer);
        return 1;
    }

    /* Point `current` at a corrupted snapshot v3 -> fresh renderer must fall back to `previous` (v1). */
    (void)snprintf(bad_snap_dir, sizeof(bad_snap_dir), "%s/by-hash/33333333333333333333333333333333", store_dir);
    if (cb_write_file_atomic(bad_snap_dir, "snapshot.cb", corrupt_bytes, sizeof(corrupt_bytes)) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", "33333333333333333333333333333333") != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    cb_renderer_free(&renderer);

    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "pi4-counter") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 300ULL) != CB_RENDER_OK ||
        strcmp(renderer.active_hash, snap_v1.sha256_hex) != 0) {
        cb_renderer_free(&renderer);
        return 1;
    }
    cb_renderer_free(&renderer);
    return 0;
}

/* TPL-005, TPL-006, RND-003, RND-007: Auto-pagination, playlist cycling, and cut/fade/slide transitions. */
static int test_autopagination_and_transitions(void)
{
    char store_dir[CB_MAX_PATH_LEN];
    cb_snapshot_t snap;
    cb_renderer_t renderer;
    uint64_t hash_p0;
    uint64_t hash_fade_mid;
    uint64_t hash_p1;
    uint64_t hash_slide_mid;

    make_temp_dir(store_dir, sizeof(store_dir), "pagination-transitions");
    /* 10 items with repeater capacity 4 -> 3 sub-pages (4, 4, 2 items). */
    populate_sample_snapshot(&snap, "aaaa1111aaaa1111aaaa1111aaaa1111", 10ULL, 10U);
    if (cb_snapshot_save_to_store(store_dir, &snap) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", snap.sha256_hex) != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "pi5-bar") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 1000ULL) != CB_RENDER_OK || renderer.subpage_total != 3U) {
        cb_renderer_free(&renderer);
        return 1;
    }
    if (renderer.subpages[0].item_count != 4U || renderer.subpages[1].item_count != 4U ||
        renderer.subpages[2].item_count != 2U) {
        cb_renderer_free(&renderer);
        return 1;
    }

    (void)cb_renderer_render_frame(&renderer, 1000ULL);
    hash_p0 = cb_surface_hash(&renderer.frame);

    /* Advance past 5000 ms page duration -> triggers 300 ms fade transition to sub-page 1. */
    (void)cb_renderer_tick(&renderer, 6000ULL, 720U);
    if (renderer.active_subpage != 1U || !renderer.in_transition) {
        cb_renderer_free(&renderer);
        return 1;
    }
    (void)cb_renderer_render_frame(&renderer, 6150ULL);
    hash_fade_mid = cb_surface_hash(&renderer.frame);

    /* Complete fade transition at 6350 ms. */
    (void)cb_renderer_render_frame(&renderer, 6350ULL);
    hash_p1 = cb_surface_hash(&renderer.frame);

    /* Verify slide transition produces a distinct intermediate frame as well (RND-003). */
    cb_surface_transition(&renderer.frame, &renderer.prev_frame, &renderer.next_frame, CB_TRANSITION_SLIDE, 150U, 300U);
    hash_slide_mid = cb_surface_hash(&renderer.frame);
    cb_renderer_free(&renderer);

    return (hash_p0 != hash_fade_mid && hash_fade_mid != hash_p1 && hash_p0 != hash_p1 &&
            hash_slide_mid != hash_fade_mid && hash_slide_mid != hash_p0 && hash_slide_mid != hash_p1)
               ? 0
               : 1;
}

/* RND-002, RND-004, RND-007, RND-008, RND-009, STO-003, TPL-008: Rotations, cache, live state, schedules, identify. */
static int test_rotations_cache_live_state_and_identify(void)
{
    char store_dir[CB_MAX_PATH_LEN];
    cb_snapshot_t snap;
    cb_renderer_t renderer;
    uint32_t icon_pixels[16];
    uint64_t rot0_hash;
    uint64_t rot90_hash;
    uint64_t rot180_hash;
    uint64_t rot270_hash;
    uint64_t sold_out_hash;
    uint64_t announce_hash;
    uint64_t identify_hash;
    uint32_t top_left_0;
    uint32_t bot_right_180;

    for (size_t i = 0; i < 16U; i++) {
        icon_pixels[i] = 0x003a7d44U;
    }
    make_temp_dir(store_dir, sizeof(store_dir), "live-state-cache");
    populate_sample_snapshot(&snap, "bbbb2222bbbb2222bbbb2222bbbb2222", 20ULL, 4U);
    snap.sold_out_mode = CB_SOLD_OUT_BADGE;
    (void)snprintf(snap.pages[0].items[0].image_hash, sizeof(snap.pages[0].items[0].image_hash),
                   "deadbeef000011112222333344445555");

    if (cb_snapshot_save_to_store(store_dir, &snap) != CB_RENDER_OK ||
        cb_render_cache_save_image(store_dir, snap.sha256_hex, "deadbeef000011112222333344445555", 4U, 4U,
                                   icon_pixels) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", snap.sha256_hex) != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_init(&renderer, CB_DEFAULT_WIDTH, CB_DEFAULT_HEIGHT, "zero2w-menu") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 0ULL) != CB_RENDER_OK || renderer.cached_image_count != 1U) {
        cb_renderer_free(&renderer);
        return 1;
    }

    /* Render at 0, 90, 180, and 270 degrees and verify whole-frame rotation (RND-002). */
    renderer.rotation = CB_ROTATION_0;
    (void)cb_renderer_render_frame(&renderer, 0ULL);
    rot0_hash = cb_surface_hash(&renderer.frame);
    top_left_0 = renderer.frame.pixels[(36U * CB_DEFAULT_WIDTH) + 48U];

    renderer.rotation = CB_ROTATION_90;
    (void)cb_renderer_render_frame(&renderer, 0ULL);
    rot90_hash = cb_surface_hash(&renderer.frame);

    renderer.rotation = CB_ROTATION_180;
    (void)cb_renderer_render_frame(&renderer, 0ULL);
    rot180_hash = cb_surface_hash(&renderer.frame);
    bot_right_180 =
        renderer.frame.pixels[((CB_DEFAULT_HEIGHT - 1U - 36U) * CB_DEFAULT_WIDTH) + (CB_DEFAULT_WIDTH - 1U - 48U)];

    renderer.rotation = CB_ROTATION_270;
    (void)cb_renderer_render_frame(&renderer, 0ULL);
    rot270_hash = cb_surface_hash(&renderer.frame);

    if (top_left_0 != bot_right_180 || rot0_hash == rot90_hash || rot0_hash == rot180_hash ||
        rot90_hash == rot270_hash) {
        cb_renderer_free(&renderer);
        return 1;
    }

    renderer.rotation = CB_ROTATION_0;
    /* Live sold-out toggle without snapshot reload (RND-008, TPL-008). */
    if (cb_renderer_set_sold_out(&renderer, "item-0", true) != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }
    (void)cb_renderer_render_frame(&renderer, 10ULL);
    sold_out_hash = cb_surface_hash(&renderer.frame);

    /* Live announcement toggle (RND-008). */
    cb_renderer_set_announcement(&renderer, "Happy Hour 16:00-18:00!", true);
    (void)cb_renderer_render_frame(&renderer, 20ULL);
    announce_hash = cb_surface_hash(&renderer.frame);

    /* Identify overlay (RND-009). */
    cb_renderer_set_identify(&renderer, true, "zero2w-menu", "Screen Left", "192.168.1.42");
    (void)cb_renderer_render_frame(&renderer, 30ULL);
    identify_hash = cb_surface_hash(&renderer.frame);
    cb_renderer_free(&renderer);

    return (rot0_hash != sold_out_hash && sold_out_hash != announce_hash && announce_hash != identify_hash) ? 0 : 1;
}

/* RND-007, TIME-007: Local daypart/schedule evaluation when agent is absent. */
static int test_schedule_daypart_switching(void)
{
    char store_dir[CB_MAX_PATH_LEN];
    cb_snapshot_t snap;
    cb_renderer_t renderer;

    make_temp_dir(store_dir, sizeof(store_dir), "daypart-schedule");
    populate_sample_snapshot(&snap, "cccc3333cccc3333cccc3333cccc3333", 30ULL, 2U);
    snap.page_count = 2U;
    /* Page 0: Breakfast 06:00..11:00 (360..660). */
    snap.pages[0].schedule_start_min = 360U;
    snap.pages[0].schedule_end_min = 660U;
    /* Page 1: Lunch 11:00..15:00 (660..900). */
    snap.pages[1] = snap.pages[0];
    (void)snprintf(snap.pages[1].id, sizeof(snap.pages[1].id), "page-lunch");
    (void)snprintf(snap.pages[1].title, sizeof(snap.pages[1].title), "LUNCH MENU");
    snap.pages[1].schedule_start_min = 660U;
    snap.pages[1].schedule_end_min = 900U;

    if (cb_snapshot_save_to_store(store_dir, &snap) != CB_RENDER_OK ||
        cb_snapshot_set_pointer_atomic(store_dir, "current", snap.sha256_hex) != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_init(&renderer, 320U, 180U, "pi4-dining") != CB_RENDER_OK) {
        return 1;
    }
    if (cb_renderer_load_from_store(&renderer, store_dir, 0ULL) != CB_RENDER_OK) {
        cb_renderer_free(&renderer);
        return 1;
    }

    /* At 08:00 (480 min) -> Page 0 (Breakfast) is active. */
    (void)cb_renderer_tick(&renderer, 100ULL, 480U);
    if (renderer.subpages[renderer.active_subpage].page_index != 0U) {
        cb_renderer_free(&renderer);
        return 1;
    }
    /* Toggling sold-out must preserve the 08:00 schedule context. */
    (void)cb_renderer_set_sold_out(&renderer, "item-0", true);
    if (renderer.subpages[renderer.active_subpage].page_index != 0U) {
        cb_renderer_free(&renderer);
        return 1;
    }

    /* At 12:30 (750 min) -> Page 1 (Lunch) becomes active automatically. */
    (void)cb_renderer_tick(&renderer, 200ULL, 750U);
    if (renderer.subpages[renderer.active_subpage].page_index != 1U) {
        cb_renderer_free(&renderer);
        return 1;
    }
    cb_renderer_free(&renderer);
    return 0;
}

/* RND-012, RND-013, RND-014: Shipped OFL fonts, tabular numerals for prices, and automatic glyph fallback. */
static int test_fonts_and_glyph_fallback(void)
{
    cb_glyph_info_t price_digit;
    cb_glyph_info_t greek_glyph;
    cb_glyph_info_t cjk_glyph;
    uint32_t cp = 0U;
    /* Greek Omega (U+03A9 = 0xCE 0xA9) and CJK 'tea' (U+8336 = 0xE8 0x8C 0xB6). */
    const char *greek_utf8 = "\xce\xa9";
    const char *cjk_utf8 = "\xe8\x8c\xb6";

    /* Price nodes must use Inter with tabular numerals (RND-013). */
    price_digit = cb_font_resolve_glyph(CB_FONT_PLAYFAIR_DISPLAY, (uint32_t)'7', true, false);
    if (price_digit.resolved_font != CB_FONT_INTER || !price_digit.tabular_numeral) {
        return 1;
    }

    /* Missing glyph in Oswald (Greek Omega) must fall back automatically to Noto Sans (RND-014). */
    if (cb_utf8_decode_next(greek_utf8, strlen(greek_utf8), &cp) != 2U || cp != 0x03a9U) {
        return 1;
    }
    greek_glyph = cb_font_resolve_glyph(CB_FONT_OSWALD, cp, false, false);
    if (greek_glyph.resolved_font != CB_FONT_NOTO_SANS || !greek_glyph.used_fallback) {
        return 1;
    }

    /* CJK glyph missing from both primary and Noto Sans falls back to uploaded custom font (RND-014). */
    if (cb_utf8_decode_next(cjk_utf8, strlen(cjk_utf8), &cp) != 3U || cp != 0x8336U) {
        return 1;
    }
    cjk_glyph = cb_font_resolve_glyph(CB_FONT_INTER, cp, false, true);
    if (cjk_glyph.resolved_font != CB_FONT_CUSTOM_FALLBACK || !cjk_glyph.used_fallback) {
        return 1;
    }
    return 0;
}

int main(void)
{
    if (test_starting_and_setup_screens() != 0) {
        (void)fputs("FAIL: test_starting_and_setup_screens\n", stderr);
        return 1;
    }
    if (test_atomic_snapshot_and_fallback() != 0) {
        (void)fputs("FAIL: test_atomic_snapshot_and_fallback\n", stderr);
        return 1;
    }
    if (test_autopagination_and_transitions() != 0) {
        (void)fputs("FAIL: test_autopagination_and_transitions\n", stderr);
        return 1;
    }
    if (test_rotations_cache_live_state_and_identify() != 0) {
        (void)fputs("FAIL: test_rotations_cache_live_state_and_identify\n", stderr);
        return 1;
    }
    if (test_schedule_daypart_switching() != 0) {
        (void)fputs("FAIL: test_schedule_daypart_switching\n", stderr);
        return 1;
    }
    if (test_fonts_and_glyph_fallback() != 0) {
        (void)fputs("FAIL: test_fonts_and_glyph_fallback\n", stderr);
        return 1;
    }
    return 0;
}
