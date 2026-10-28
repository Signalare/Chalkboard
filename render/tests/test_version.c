/* SPDX-License-Identifier: MIT OR Apache-2.0 */

#include <stdio.h>
#include <string.h>

#include "chalkboard/render.h"

int main(void)
{
    const char *version = cb_render_version();

    if (version == NULL || strlen(version) == 0) {
        (void)fputs("cb_render_version() returned an empty version\n", stderr);
        return 1;
    }
    return 0;
}
