/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * cb-tryboot-reboot: reboot once with the firmware's one-shot tryboot flag.
 * The Raspberry Pi firmware driver turns the "0 tryboot" restart argument
 * into the tryboot flag for the next boot only.
 */

#include <linux/reboot.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
    sync();
    if (syscall(SYS_reboot, LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2, LINUX_REBOOT_CMD_RESTART2, "0 tryboot") != 0) {
        perror("reboot");
        return 1;
    }
    return 0;
}
