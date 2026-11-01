// SPDX-License-Identifier: MIT OR Apache-2.0
/*
 * Chalkboard Hardware-in-the-Loop CEC Verification Tool.
 *
 * Exercises the Linux kernel CEC API (/dev/cec0) on physical displays (CEC-001..CEC-006, TEST-007):
 * - Logical address allocation (Playback device)
 * - Physical address querying
 * - OSD Name announcement ("Chalkboard")
 * - Image View On (0x04)
 * - Active Source (0x82)
 * - Standby (0x36)
 * - Power Status polling (0x8f / 0x90)
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/ioctl.h>
#include <sys/time.h>
#include <errno.h>
#include <linux/cec.h>
#include <linux/cec-funcs.h>

static double get_time_ms(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (tv.tv_sec * 1000.0) + (tv.tv_usec / 1000.0);
}

static const char *power_status_str(__u8 pwr) {
    switch (pwr) {
        case CEC_OP_POWER_STATUS_ON: return "ON";
        case CEC_OP_POWER_STATUS_STANDBY: return "STANDBY";
        case CEC_OP_POWER_STATUS_TO_ON: return "TRANSITION_TO_ON";
        case CEC_OP_POWER_STATUS_TO_STANDBY: return "TRANSITION_TO_STANDBY";
        default: return "UNKNOWN";
    }
}

int main(int argc, char **argv) {
    const char *action = (argc > 1) ? argv[1] : "probe";
    const char *devpath = (argc > 2) ? argv[2] : "/dev/cec0";

    printf("[CEC-HIL] Opening CEC adapter %s...\n", devpath);
    int fd = open(devpath, O_RDWR);
    if (fd < 0) {
        perror("[CEC-HIL] open");
        return 1;
    }

    struct cec_caps caps;
    memset(&caps, 0, sizeof(caps));
    if (ioctl(fd, CEC_ADAP_G_CAPS, &caps) < 0) {
        perror("[CEC-HIL] CEC_ADAP_G_CAPS");
        close(fd);
        return 1;
    }
    printf("[CEC-HIL] Driver: %s, Name: %s, Caps: 0x%08x\n", caps.driver, caps.name, caps.capabilities);

    __u16 phys_addr = 0;
    if (ioctl(fd, CEC_ADAP_G_PHYS_ADDR, &phys_addr) < 0) {
        perror("[CEC-HIL] CEC_ADAP_G_PHYS_ADDR");
    } else {
        printf("[CEC-HIL] Physical Address: %x.%x.%x.%x (raw: 0x%04x)\n",
               (phys_addr >> 12) & 0xf, (phys_addr >> 8) & 0xf,
               (phys_addr >> 4) & 0xf, phys_addr & 0xf, phys_addr);
    }

    struct cec_log_addrs laddrs;
    memset(&laddrs, 0, sizeof(laddrs));
    if (ioctl(fd, CEC_ADAP_G_LOG_ADDRS, &laddrs) < 0) {
        perror("[CEC-HIL] CEC_ADAP_G_LOG_ADDRS");
    }

    if (laddrs.num_log_addrs == 0) {
        printf("[CEC-HIL] Configuring logical address as Playback Device (OSD: Chalkboard)...\n");
        memset(&laddrs, 0, sizeof(laddrs));
        laddrs.cec_version = CEC_OP_CEC_VERSION_1_4;
        laddrs.num_log_addrs = 1;
        laddrs.log_addr_type[0] = CEC_LOG_ADDR_TYPE_PLAYBACK;
        laddrs.all_device_types[0] = CEC_OP_ALL_DEVTYPE_PLAYBACK;
        laddrs.primary_device_type[0] = CEC_OP_PRIM_DEVTYPE_PLAYBACK;
        strncpy(laddrs.osd_name, "Chalkboard", sizeof(laddrs.osd_name) - 1);

        if (ioctl(fd, CEC_ADAP_S_LOG_ADDRS, &laddrs) < 0) {
            perror("[CEC-HIL] CEC_ADAP_S_LOG_ADDRS");
            close(fd);
            return 1;
        }
        printf("[CEC-HIL] Claimed Logical Address: 0x%x (mask: 0x%04x)\n",
               laddrs.log_addr[0], laddrs.log_addr_mask);
    } else {
        printf("[CEC-HIL] Already configured: Log Addr: 0x%x (OSD: %s)\n",
               laddrs.log_addr[0], laddrs.osd_name);
    }

    __u8 my_la = laddrs.log_addr[0];
    if (my_la == CEC_LOG_ADDR_INVALID) {
        my_la = 0x4; // Fallback to Playback 1
    }

    if (strcmp(action, "probe") == 0 || strcmp(action, "status") == 0) {
        printf("[CEC-HIL] Querying TV power status (0x8f Give Device Power Status)...\n");
        struct cec_msg msg;
        cec_msg_init(&msg, my_la, CEC_LOG_ADDR_TV);
        cec_msg_give_device_power_status(&msg, 1); // request reply

        double t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT status query");
        } else {
            double elapsed = get_time_ms() - t0;
            printf("[CEC-HIL] Transmit result: tx_status=0x%x, rx_status=0x%x (took %.2f ms)\n",
                   msg.tx_status, msg.rx_status, elapsed);

            if (msg.rx_status & CEC_RX_STATUS_OK) {
                __u8 pwr_status = 0;
                cec_ops_report_power_status(&msg, &pwr_status);
                printf("[CEC-HIL] TV Power Status: %s (0x%02x)\n", power_status_str(pwr_status), pwr_status);
            } else {
                printf("[CEC-HIL] No immediate reply; waiting on CEC_RECEIVE...\n");
                // Wait briefly for incoming Report Power Status
                for (int i = 0; i < 10; i++) {
                    struct cec_msg rx_msg;
                    memset(&rx_msg, 0, sizeof(rx_msg));
                    rx_msg.timeout = 500;
                    if (ioctl(fd, CEC_RECEIVE, &rx_msg) == 0) {
                        if (rx_msg.msg[1] == CEC_MSG_REPORT_POWER_STATUS) {
                            __u8 pwr_status = rx_msg.msg[2];
                            printf("[CEC-HIL] Received Report Power Status: %s (0x%02x)\n",
                                   power_status_str(pwr_status), pwr_status);
                            break;
                        }
                    }
                    usleep(100000);
                }
            }
        }
    } else if (strcmp(action, "on") == 0) {
        printf("[CEC-HIL] Sending Image View On (0x04) to TV...\n");
        struct cec_msg msg;
        cec_msg_init(&msg, my_la, CEC_LOG_ADDR_TV);
        cec_msg_image_view_on(&msg);

        double t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT Image View On");
        } else {
            printf("[CEC-HIL] Image View On sent! tx_status=0x%x (took %.2f ms)\n",
                   msg.tx_status, get_time_ms() - t0);
        }

        usleep(200000); // 200 ms inter-frame pause

        printf("[CEC-HIL] Broadcasting Active Source (%x.%x.%x.%x)...\n",
               (phys_addr >> 12) & 0xf, (phys_addr >> 8) & 0xf,
               (phys_addr >> 4) & 0xf, phys_addr & 0xf);
        struct cec_msg act_msg;
        cec_msg_init(&act_msg, my_la, CEC_LOG_ADDR_BROADCAST);
        cec_msg_active_source(&act_msg, phys_addr);

        t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &act_msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT Active Source");
        } else {
            printf("[CEC-HIL] Active Source sent! tx_status=0x%x (took %.2f ms)\n",
                   act_msg.tx_status, get_time_ms() - t0);
        }
    } else if (strcmp(action, "active") == 0) {
        printf("[CEC-HIL] Broadcasting Active Source (%x.%x.%x.%x)...\n",
               (phys_addr >> 12) & 0xf, (phys_addr >> 8) & 0xf,
               (phys_addr >> 4) & 0xf, phys_addr & 0xf);
        struct cec_msg act_msg;
        cec_msg_init(&act_msg, my_la, CEC_LOG_ADDR_BROADCAST);
        cec_msg_active_source(&act_msg, phys_addr);

        double t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &act_msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT Active Source");
        } else {
            printf("[CEC-HIL] Active Source sent! tx_status=0x%x (took %.2f ms)\n",
                   act_msg.tx_status, get_time_ms() - t0);
        }
    } else if (strcmp(action, "standby") == 0) {
        printf("[CEC-HIL] Sending Standby (0x36) to TV...\n");
        struct cec_msg msg;
        cec_msg_init(&msg, my_la, CEC_LOG_ADDR_TV);
        cec_msg_standby(&msg);

        double t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT Standby");
        } else {
            printf("[CEC-HIL] Standby sent! tx_status=0x%x (took %.2f ms)\n",
                   msg.tx_status, get_time_ms() - t0);
        }
    } else if (strcmp(action, "osd") == 0) {
        printf("[CEC-HIL] Setting OSD Name to Chalkboard...\n");
        struct cec_msg msg;
        cec_msg_init(&msg, my_la, CEC_LOG_ADDR_TV);
        cec_msg_set_osd_name(&msg, "Chalkboard");

        double t0 = get_time_ms();
        if (ioctl(fd, CEC_TRANSMIT, &msg) < 0) {
            perror("[CEC-HIL] CEC_TRANSMIT Set OSD Name");
        } else {
            printf("[CEC-HIL] Set OSD Name sent! tx_status=0x%x (took %.2f ms)\n",
                   msg.tx_status, get_time_ms() - t0);
        }
    } else {
        fprintf(stderr, "Unknown action: %s (expected probe, status, on, active, standby, osd)\n", action);
        close(fd);
        return 1;
    }

    close(fd);
    printf("[CEC-HIL] Completed action %s successfully.\n", action);
    return 0;
}
