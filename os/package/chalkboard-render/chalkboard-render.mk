# SPDX-License-Identifier: MIT OR Apache-2.0
################################################################################
#
# chalkboard-render
#
################################################################################

CHALKBOARD_RENDER_VERSION = 0.0.0
CHALKBOARD_RENDER_SITE = $(BR2_EXTERNAL_CHALKBOARD_PATH)/../render
CHALKBOARD_RENDER_SITE_METHOD = local
CHALKBOARD_RENDER_LICENSE = MIT or Apache-2.0

define CHALKBOARD_RENDER_BUILD_CMDS
	$(TARGET_CC) $(TARGET_CFLAGS) -O2 -std=c11 \
		-D_POSIX_C_SOURCE=200809L -DCB_RENDER_VERSION=\"$(CHALKBOARD_RENDER_VERSION)\" \
		-I$(@D)/include \
		$(@D)/src/core/version.c \
		$(@D)/src/core/surface.c \
		$(@D)/src/core/utf8_font.c \
		$(@D)/src/core/snapshot.c \
		$(@D)/src/core/renderer.c \
		$(@D)/src/backends/drm/main.c \
		-o $(@D)/chalkboard-render
endef

define CHALKBOARD_RENDER_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/chalkboard-render $(TARGET_DIR)/usr/bin/chalkboard-render
endef

$(eval $(generic-package))
