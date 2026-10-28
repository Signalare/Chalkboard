/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * chalkboard-render: draws the current snapshot via DRM/KMS (SPEC §7).
 * Placeholder entry point until the renderer core lands (#11).
 */

#include <stdio.h>

#include "chalkboard/render.h"

int main(void)
{
    printf("chalkboard-render %s\n", cb_render_version());
    return 0;
}
