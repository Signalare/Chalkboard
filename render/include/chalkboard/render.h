/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#ifndef CHALKBOARD_RENDER_H
#define CHALKBOARD_RENDER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Default 1080p output dimensions (RND-002). */
#define CB_DEFAULT_WIDTH  1920U
#define CB_DEFAULT_HEIGHT 1080U

/** Optional 4K output dimensions on BCM2712 (RND-015). */
#define CB_4K_WIDTH  3840U
#define CB_4K_HEIGHT 2160U

/** Limits for deterministic bounded memory use (RND-011). */
#define CB_MAX_HASH_HEX      65U
#define CB_MAX_TEXT_LEN      128U
#define CB_MAX_ITEMS         32U
#define CB_MAX_PAGES         8U
#define CB_MAX_SUBPAGES      32U
#define CB_MAX_CACHED_IMAGES 8U
#define CB_MAX_PATH_LEN      512U

/** Screen rotation in degrees clockwise (RND-002). */
typedef enum { CB_ROTATION_0 = 0, CB_ROTATION_90 = 90, CB_ROTATION_180 = 180, CB_ROTATION_270 = 270 } cb_rotation_t;

/** Page-to-page transition style (RND-003). */
typedef enum { CB_TRANSITION_CUT = 0, CB_TRANSITION_FADE = 1, CB_TRANSITION_SLIDE = 2 } cb_transition_t;

/** Sold-out item presentation mode (TPL-008, RND-008). */
typedef enum { CB_SOLD_OUT_HIDE = 0, CB_SOLD_OUT_DIM = 1, CB_SOLD_OUT_BADGE = 2 } cb_sold_out_mode_t;

/** Image scaling policy (RND-004). */
typedef enum { CB_SCALE_COVER = 0, CB_SCALE_CONTAIN = 1, CB_SCALE_STRETCH = 2 } cb_image_scale_t;

/** Shipped SIL OFL and custom fallback font families (RND-012, RND-013, RND-014). */
typedef enum {
    CB_FONT_INTER = 0,
    CB_FONT_NOTO_SANS = 1,
    CB_FONT_OSWALD = 2,
    CB_FONT_PLAYFAIR_DISPLAY = 3,
    CB_FONT_CUSTOM_FALLBACK = 4,
} cb_font_family_t;

/** High-level screen state when rendering (BOOT-005, BOOT-006, RND-007). */
typedef enum { CB_SCREEN_MODE_STARTING = 0, CB_SCREEN_MODE_SETUP = 1, CB_SCREEN_MODE_CONTENT = 2 } cb_screen_mode_t;

/** Error codes returned by the renderer core. */
typedef enum {
    CB_RENDER_OK = 0,
    CB_RENDER_ERR_INVALID_ARG = -1,
    CB_RENDER_ERR_IO = -2,
    CB_RENDER_ERR_CORRUPT = -3,
    CB_RENDER_ERR_NO_SNAPSHOT = -4,
    CB_RENDER_ERR_NOMEM = -5,
} cb_render_err_t;

/** Resolved glyph source showing automatic font fallback (RND-013, RND-014). */
typedef struct {
    uint32_t codepoint;
    cb_font_family_t resolved_font;
    uint32_t advance_px;
    bool used_fallback;
    bool tabular_numeral;
} cb_glyph_info_t;

/** Pre-decoded RGBA/XRGB image asset in the render cache (STO-003, BOOT-005). */
typedef struct {
    char sha256_hex[CB_MAX_HASH_HEX];
    uint32_t width;
    uint32_t height;
    uint32_t *pixels;
} cb_cached_image_t;

/** One menu item inside a compiled snapshot page (TPL-004, DATA-002). */
typedef struct {
    char id[CB_MAX_TEXT_LEN];
    char name[CB_MAX_TEXT_LEN];
    char description[CB_MAX_TEXT_LEN];
    char price_primary[CB_MAX_TEXT_LEN];
    char price_secondary[CB_MAX_TEXT_LEN];
    char badge[CB_MAX_TEXT_LEN];
    char image_hash[CB_MAX_HASH_HEX];
    bool sold_out;
} cb_snapshot_item_t;

/** One page in a compiled snapshot (TPL-002, TPL-005, TPL-006, TIME-007). */
typedef struct {
    char id[CB_MAX_TEXT_LEN];
    char title[CB_MAX_TEXT_LEN];
    cb_font_family_t heading_font;
    cb_font_family_t body_font;
    uint32_t bg_color;
    uint32_t heading_color;
    uint32_t text_color;
    uint32_t accent_color;
    uint32_t duration_ms;
    cb_transition_t transition;
    uint32_t transition_ms;
    uint32_t repeater_capacity;
    uint16_t schedule_start_min;
    uint16_t schedule_end_min;
    uint32_t item_count;
    cb_snapshot_item_t items[CB_MAX_ITEMS];
} cb_snapshot_page_t;

/** Complete compiled snapshot loaded from disk (STO-002, STO-003, STO-004). */
typedef struct {
    uint32_t schema_version;
    uint64_t publication_id;
    char sha256_hex[CB_MAX_HASH_HEX];
    uint32_t logical_width;
    uint32_t logical_height;
    cb_rotation_t rotation;
    cb_sold_out_mode_t sold_out_mode;
    char sold_out_badge[CB_MAX_TEXT_LEN];
    uint8_t sold_out_dim_alpha;
    uint32_t page_count;
    cb_snapshot_page_t pages[CB_MAX_PAGES];
} cb_snapshot_t;

/** Expanded sub-page produced by auto-pagination (TPL-006). */
typedef struct {
    uint32_t page_index;
    uint32_t subpage_index;
    uint32_t subpage_count;
    uint32_t item_start;
    uint32_t item_count;
    uint32_t duration_ms;
    cb_transition_t transition;
    uint32_t transition_ms;
} cb_subpage_slot_t;

/** Identify overlay state (RND-009). */
typedef struct {
    bool enabled;
    char node_name[CB_MAX_TEXT_LEN];
    char screen_name[CB_MAX_TEXT_LEN];
    char ip_address[CB_MAX_TEXT_LEN];
} cb_identify_state_t;

/** Platform-independent software ARGB8888/XRGB8888 surface (RND-005, RND-017). */
typedef struct {
    uint32_t width;
    uint32_t height;
    uint32_t *pixels;
} cb_surface_t;

/** Renderer instance state (RND-005, RND-007). */
typedef struct {
    uint32_t output_width;
    uint32_t output_height;
    cb_rotation_t rotation;
    cb_screen_mode_t mode;
    bool first_boot_setup;
    bool custom_fallback_loaded;
    char node_name[CB_MAX_TEXT_LEN];
    char setup_ssid[CB_MAX_TEXT_LEN];
    char setup_url[CB_MAX_TEXT_LEN];
    char active_hash[CB_MAX_HASH_HEX];
    char fallback_hash[CB_MAX_HASH_HEX];
    bool has_snapshot;
    cb_snapshot_t snapshot;
    uint32_t subpage_total;
    cb_subpage_slot_t subpages[CB_MAX_SUBPAGES];
    uint32_t active_subpage;
    uint32_t previous_subpage;
    uint16_t current_minute_of_day;
    uint64_t page_entered_ms;
    bool in_transition;
    uint64_t transition_started_ms;
    bool announcement_active;
    char announcement_text[CB_MAX_TEXT_LEN];
    cb_identify_state_t identify;
    uint32_t cached_image_count;
    cb_cached_image_t cached_images[CB_MAX_CACHED_IMAGES];
    uint32_t fallback_glyph_count;
    cb_surface_t frame;
    cb_surface_t prev_frame;
    cb_surface_t next_frame;
} cb_renderer_t;

/** Returns the renderer version as "MAJOR.MINOR.PATCH". */
const char *cb_render_version(void);

/** Surface lifecycle and drawing primitives. */
cb_render_err_t cb_surface_init(cb_surface_t *surface, uint32_t width, uint32_t height);
void cb_surface_free(cb_surface_t *surface);
void cb_surface_clear(cb_surface_t *surface, uint32_t xrgb);
void cb_surface_fill_rect(cb_surface_t *surface, uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint32_t xrgb,
                          uint8_t alpha);
uint64_t cb_surface_hash(const cb_surface_t *surface);

/** Rotates logical canvas coordinates into physical output coordinates (RND-002). */
void cb_map_rotated_rect(cb_rotation_t rotation, uint32_t canvas_w, uint32_t canvas_h, uint32_t x, uint32_t y,
                         uint32_t w, uint32_t h, uint32_t *out_x, uint32_t *out_y, uint32_t *out_w, uint32_t *out_h);

/** Blends two surfaces according to a transition style and progress (RND-003). */
void cb_surface_transition(cb_surface_t *dest, const cb_surface_t *from, const cb_surface_t *to,
                           cb_transition_t transition, uint32_t elapsed_ms, uint32_t duration_ms);

/** UTF-8 decoding, OFL font catalog and automatic glyph fallback (RND-012, RND-013, RND-014). */
size_t cb_utf8_decode_next(const char *text, size_t len, uint32_t *out_codepoint);
cb_glyph_info_t cb_font_resolve_glyph(cb_font_family_t preferred_font, uint32_t codepoint, bool is_price_node,
                                      bool has_custom_fallback);
uint32_t cb_draw_text_utf8(cb_surface_t *surface, uint32_t x, uint32_t y, const char *text, cb_font_family_t font,
                           uint32_t scale, uint32_t xrgb, uint8_t alpha, bool is_price_node, bool has_custom_fallback);

/** Atomic file and snapshot/render-cache operations (STO-002, STO-003, STO-004). */
cb_render_err_t cb_write_file_atomic(const char *dir_path, const char *file_name, const uint8_t *data, size_t len);
cb_render_err_t cb_snapshot_save_to_store(const char *snapshots_root, const cb_snapshot_t *snapshot);
cb_render_err_t cb_snapshot_set_pointer_atomic(const char *snapshots_root, const char *pointer_name,
                                               const char *sha256_hex);
cb_render_err_t cb_render_cache_save_image(const char *snapshots_root, const char *snapshot_hash,
                                           const char *image_hash, uint32_t width, uint32_t height,
                                           const uint32_t *pixels);

/** Renderer lifecycle, snapshot loading, live state and frame rendering. */
cb_render_err_t cb_renderer_init(cb_renderer_t *renderer, uint32_t output_width, uint32_t output_height,
                                 const char *node_name);
void cb_renderer_free(cb_renderer_t *renderer);
void cb_renderer_set_setup_info(cb_renderer_t *renderer, bool first_boot_setup, const char *ssid, const char *url);
void cb_renderer_set_custom_fallback_font(cb_renderer_t *renderer, bool enabled);
cb_render_err_t cb_renderer_load_from_store(cb_renderer_t *renderer, const char *snapshots_root, uint64_t now_ms);
cb_render_err_t cb_renderer_set_sold_out(cb_renderer_t *renderer, const char *item_id, bool sold_out);
void cb_renderer_set_announcement(cb_renderer_t *renderer, const char *text, bool active);
void cb_renderer_set_identify(cb_renderer_t *renderer, bool enabled, const char *node_name, const char *screen_name,
                              const char *ip_address);
cb_render_err_t cb_renderer_tick(cb_renderer_t *renderer, uint64_t now_ms, uint16_t local_minute_of_day);
cb_render_err_t cb_renderer_render_frame(cb_renderer_t *renderer, uint64_t now_ms);

#ifdef __cplusplus
}
#endif

#endif /* CHALKBOARD_RENDER_H */
