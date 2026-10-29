# SPDX-License-Identifier: MIT OR Apache-2.0
################################################################################
#
# cb-rpi-eeprom
#
################################################################################

# Release v2026.09.25-2712 of https://github.com/raspberrypi/rpi-eeprom
CB_RPI_EEPROM_VERSION = ded1e9332bd996f6adc46e10d22d9ef0298b1882
CB_RPI_EEPROM_SITE = $(call github,raspberrypi,rpi-eeprom,$(CB_RPI_EEPROM_VERSION))
CB_RPI_EEPROM_LICENSE = BSD-3-Clause (scripts), Raspberry Pi binary licence (firmware)
CB_RPI_EEPROM_LICENSE_FILES = LICENSE
CB_RPI_EEPROM_INSTALL_TARGET = NO
CB_RPI_EEPROM_INSTALL_IMAGES = YES

# Stable ("default") channel bootloader for BCM2711.
CB_RPI_EEPROM_IMAGE = firmware-2711/default/pieeprom-2026-09-23.bin
CB_RPI_EEPROM_CONFIG = $(call qstrip,$(BR2_PACKAGE_CB_RPI_EEPROM_CONFIG_FILE))

define CB_RPI_EEPROM_BUILD_CMDS
	python3 $(@D)/rpi-eeprom-config --config $(CB_RPI_EEPROM_CONFIG) \
		--out $(@D)/pieeprom.upd $(@D)/$(CB_RPI_EEPROM_IMAGE)
	cd $(@D) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) ./rpi-eeprom-digest \
		-i pieeprom.upd -o pieeprom.sig
endef

define CB_RPI_EEPROM_INSTALL_IMAGES_CMDS
	$(INSTALL) -D -m 0644 $(@D)/pieeprom.upd $(BINARIES_DIR)/rpi-eeprom/pieeprom.upd
	$(INSTALL) -D -m 0644 $(@D)/pieeprom.sig $(BINARIES_DIR)/rpi-eeprom/pieeprom.sig
	$(INSTALL) -D -m 0644 $(@D)/firmware-2711/default/recovery.bin \
		$(BINARIES_DIR)/rpi-eeprom/recovery.bin
endef

$(eval $(generic-package))
