/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#ifndef CHALKBOARD_RENDER_INTERNAL_H
#define CHALKBOARD_RENDER_INTERNAL_H

#include "chalkboard/render.h"

cb_render_err_t cb_snapshot_read_pointer(const char *snapshots_root, const char *pointer_name, char *out_hash,
                                         size_t out_size);
cb_render_err_t cb_snapshot_load_by_hash(const char *snapshots_root, const char *sha256_hex, cb_snapshot_t *out_snap);
cb_render_err_t cb_render_cache_load_image(const char *snapshots_root, const char *snapshot_hash,
                                           const char *image_hash, cb_cached_image_t *out_img);
void cb_cached_image_free(cb_cached_image_t *img);

#endif /* CHALKBOARD_RENDER_INTERNAL_H */
