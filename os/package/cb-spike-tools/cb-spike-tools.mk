# SPDX-License-Identifier: MIT OR Apache-2.0
################################################################################
#
# cb-spike-tools
#
################################################################################

CB_SPIKE_TOOLS_VERSION = 0.0.0
CB_SPIKE_TOOLS_SITE = $(BR2_EXTERNAL_CHALKBOARD_PATH)/package/cb-spike-tools/src
CB_SPIKE_TOOLS_SITE_METHOD = local
CB_SPIKE_TOOLS_LICENSE = MIT or Apache-2.0

define CB_SPIKE_TOOLS_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D) CC="$(TARGET_CC)" CFLAGS="$(TARGET_CFLAGS)"
endef

define CB_SPIKE_TOOLS_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/cb-firstframe $(TARGET_DIR)/usr/sbin/cb-firstframe
	$(INSTALL) -D -m 0755 $(@D)/cb-tryboot-reboot $(TARGET_DIR)/usr/sbin/cb-tryboot-reboot
	$(INSTALL) -D -m 0755 $(@D)/cb-rawpatch $(TARGET_DIR)/usr/sbin/cb-rawpatch
	$(INSTALL) -D -m 0755 $(@D)/cb-firstframe-start $(TARGET_DIR)/usr/sbin/cb-firstframe-start
	$(INSTALL) -D -m 0755 $(@D)/cb-slot $(TARGET_DIR)/usr/sbin/cb-slot
	$(INSTALL) -D -m 0755 $(@D)/cb-tryboot $(TARGET_DIR)/usr/sbin/cb-tryboot
	$(INSTALL) -D -m 0755 $(@D)/cb-commit $(TARGET_DIR)/usr/sbin/cb-commit
	$(INSTALL) -D -m 0755 $(@D)/cb-slot-offsets $(TARGET_DIR)/usr/sbin/cb-slot-offsets
	$(INSTALL) -D -m 0755 $(@D)/cb-commit-loop $(TARGET_DIR)/usr/sbin/cb-commit-loop
	$(INSTALL) -D -m 0755 $(@D)/cb-network-start $(TARGET_DIR)/usr/sbin/cb-network-start
	$(INSTALL) -D -m 0755 $(@D)/cb-reflash $(TARGET_DIR)/usr/sbin/cb-reflash
endef

$(eval $(generic-package))
