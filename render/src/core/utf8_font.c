/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include "chalkboard/render.h"

#include <string.h>

#define UNICODE_REPLACEMENT_CHAR 0xfffdU

static size_t decode_two_byte(const uint8_t *bytes, size_t len, uint32_t *out_cp)
{
    uint32_t cp;

    if (len < 2U || (bytes[1] & 0xc0U) != 0x80U) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    cp = ((uint32_t)(bytes[0] & 0x1fU) << 6U) | (uint32_t)(bytes[1] & 0x3fU);
    if (cp < 0x80U) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    *out_cp = cp;
    return 2U;
}

static size_t decode_three_byte(const uint8_t *bytes, size_t len, uint32_t *out_cp)
{
    uint32_t cp;

    if (len < 3U || (bytes[1] & 0xc0U) != 0x80U || (bytes[2] & 0xc0U) != 0x80U) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    cp = ((uint32_t)(bytes[0] & 0x0fU) << 12U) | ((uint32_t)(bytes[1] & 0x3fU) << 6U) | (uint32_t)(bytes[2] & 0x3fU);
    if (cp < 0x800U || (cp >= 0xd800U && cp <= 0xdfffU)) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    *out_cp = cp;
    return 3U;
}

static size_t decode_four_byte(const uint8_t *bytes, size_t len, uint32_t *out_cp)
{
    uint32_t cp;

    if (len < 4U || (bytes[1] & 0xc0U) != 0x80U || (bytes[2] & 0xc0U) != 0x80U || (bytes[3] & 0xc0U) != 0x80U) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    cp = ((uint32_t)(bytes[0] & 0x07U) << 18U) | ((uint32_t)(bytes[1] & 0x3fU) << 12U) |
         ((uint32_t)(bytes[2] & 0x3fU) << 6U) | (uint32_t)(bytes[3] & 0x3fU);
    if (cp < 0x10000U || cp > 0x10ffffU) {
        *out_cp = UNICODE_REPLACEMENT_CHAR;
        return 1U;
    }
    *out_cp = cp;
    return 4U;
}

size_t cb_utf8_decode_next(const char *text, size_t len, uint32_t *out_codepoint)
{
    const uint8_t *bytes = (const uint8_t *)text;
    uint8_t b0;

    if (text == NULL || len == 0U || out_codepoint == NULL || bytes[0] == 0U) {
        if (out_codepoint != NULL) {
            *out_codepoint = 0U;
        }
        return 0U;
    }
    b0 = bytes[0];
    if ((b0 & 0x80U) == 0U) {
        *out_codepoint = (uint32_t)b0;
        return 1U;
    }
    if ((b0 & 0xe0U) == 0xc0U) {
        return decode_two_byte(bytes, len, out_codepoint);
    }
    if ((b0 & 0xf0U) == 0xe0U) {
        return decode_three_byte(bytes, len, out_codepoint);
    }
    if ((b0 & 0xf8U) == 0xf0U) {
        return decode_four_byte(bytes, len, out_codepoint);
    }
    *out_codepoint = UNICODE_REPLACEMENT_CHAR;
    return 1U;
}

static bool primary_font_covers(cb_font_family_t font, uint32_t cp)
{
    if (font == CB_FONT_NOTO_SANS) {
        if ((cp >= 0x20U && cp <= 0x024fU) || (cp >= 0x0370U && cp <= 0x052fU) || (cp >= 0x2000U && cp <= 0x20cfU)) {
            return true;
        }
        return false;
    }
    /* Inter, Oswald, and Playfair Display cover basic Latin and Latin-1 Supplement. */
    if ((cp >= 0x20U && cp <= 0x7eU) || (cp >= 0xa0U && cp <= 0xffU)) {
        return true;
    }
    return false;
}

static bool noto_sans_covers(uint32_t cp)
{
    if ((cp >= 0x20U && cp <= 0x024fU) || (cp >= 0x0370U && cp <= 0x052fU) || (cp >= 0x2000U && cp <= 0x20cfU)) {
        return true;
    }
    return false;
}

cb_glyph_info_t cb_font_resolve_glyph(cb_font_family_t preferred_font, uint32_t codepoint, bool is_price_node,
                                      bool has_custom_fallback)
{
    cb_glyph_info_t info;
    bool is_digit = false;

    if (codepoint >= (uint32_t)'0' && codepoint <= (uint32_t)'9') {
        is_digit = true;
    }

    info.codepoint = codepoint;
    info.resolved_font = preferred_font;
    info.used_fallback = false;
    info.tabular_numeral = false;
    if (is_digit && (is_price_node || preferred_font == CB_FONT_INTER)) {
        info.tabular_numeral = true;
    }

    /* Price nodes always use Inter tabular numerals for digits (RND-013). */
    if (is_price_node && is_digit) {
        info.resolved_font = CB_FONT_INTER;
        info.advance_px = 8U;
        return info;
    }

    if (primary_font_covers(preferred_font, codepoint)) {
        info.advance_px = info.tabular_numeral ? 8U : 7U;
        return info;
    }

    /* Automatic glyph fallback: first Noto Sans, then uploaded custom font (RND-014). */
    if (noto_sans_covers(codepoint)) {
        info.resolved_font = CB_FONT_NOTO_SANS;
        if (preferred_font != CB_FONT_NOTO_SANS) {
            info.used_fallback = true;
        }
        info.advance_px = 8U;
        return info;
    }

    if (has_custom_fallback) {
        info.resolved_font = CB_FONT_CUSTOM_FALLBACK;
        info.used_fallback = true;
        info.advance_px = 10U;
        return info;
    }

    info.resolved_font = CB_FONT_NOTO_SANS;
    info.used_fallback = true;
    info.advance_px = 8U;
    return info;
}

static uint32_t glyph_row_pattern(uint32_t cp, cb_font_family_t font, uint32_t row)
{
    uint32_t seed;

    if (cp == (uint32_t)' ') {
        return 0U;
    }
    /* Solid border for top/bottom of digits and uppercase, plus deterministic interior bits. */
    seed = (cp * 2654435761U) ^ ((uint32_t)font * 40503U) ^ (row * 193U);
    if (row == 0U || row == 6U) {
        return 0x1eU | ((seed >> 3U) & 0x21U);
    }
    return 0x10U | ((seed >> (row & 7U)) & 0x2fU);
}

static void draw_single_glyph(cb_surface_t *surface, uint32_t x, uint32_t y, const cb_glyph_info_t *glyph,
                              uint32_t scale, uint32_t xrgb, uint8_t alpha)
{
    uint32_t px_scale = (scale == 0U) ? 1U : scale;

    for (uint32_t row = 0U; row < 7U; row++) {
        uint32_t bits = glyph_row_pattern(glyph->codepoint, glyph->resolved_font, row);
        for (uint32_t col = 0U; col < 6U; col++) {
            if (((bits >> col) & 1U) != 0U) {
                cb_surface_fill_rect(surface, x + (col * px_scale), y + (row * px_scale), px_scale, px_scale, xrgb,
                                     alpha);
            }
        }
    }
}

uint32_t cb_draw_text_utf8(cb_surface_t *surface, uint32_t x, uint32_t y, const char *text, cb_font_family_t font,
                           uint32_t scale, uint32_t xrgb, uint8_t alpha, bool is_price_node, bool has_custom_fallback)
{
    size_t len;
    size_t offset = 0U;
    uint32_t cursor_x = x;
    uint32_t fallback_count = 0U;
    uint32_t px_scale = (scale == 0U) ? 1U : scale;

    if (surface == NULL || text == NULL) {
        return 0U;
    }
    len = strlen(text);
    while (offset < len) {
        uint32_t cp = 0U;
        size_t consumed = cb_utf8_decode_next(&text[offset], len - offset, &cp);
        cb_glyph_info_t glyph;

        if (consumed == 0U || cp == 0U) {
            break;
        }
        offset += consumed;
        glyph = cb_font_resolve_glyph(font, cp, is_price_node, has_custom_fallback);
        if (glyph.used_fallback) {
            fallback_count++;
        }
        draw_single_glyph(surface, cursor_x, y, &glyph, px_scale, xrgb, alpha);
        cursor_x += glyph.advance_px * px_scale;
        if (cursor_x >= surface->width) {
            break;
        }
    }
    return fallback_count;
}
