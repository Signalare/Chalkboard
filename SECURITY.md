# Security Policy

Chalkboard handles user credentials, cluster keys and signed firmware updates,
so we take security reports seriously and appreciate responsible disclosure.

## Reporting a vulnerability

**Do not open a public issue, discussion or pull request for security problems.**

Report privately through one of:

1. **GitHub Private Vulnerability Reporting** (preferred) — go to the
   repository's **Security** tab and choose **Report a vulnerability**.
2. **Email** — security@signalare.se

Please include:

- affected component (`chalkboard-agent`, `chalkboard-render`, OS image, web app, update bundles, …)
- firmware version and, if relevant, number of nodes and network setup
- steps to reproduce or a proof of concept
- impact as you understand it
- whether you want to be credited, and how

## What to expect

| Step | Target |
|---|---|
| Acknowledgement of your report | within 3 business days |
| Initial assessment and severity | within 10 business days |
| Fix or mitigation plan | communicated as soon as it exists |
| Coordinated public disclosure | within 90 days of the report, or earlier once a fix is released |

We will keep you informed throughout, agree on a disclosure date with you, and
credit you in the advisory and changelog unless you prefer otherwise. If a
report turns out to be actively exploited, we may disclose sooner.

## Supported versions

Chalkboard is pre-1.0. Security fixes are made for the **latest minor release
only**. After 1.0 this policy will be revised.

| Version | Supported |
|---|---|
| latest 0.x | ✅ |
| older 0.x | ❌ |

## Scope

In scope: all code in this repository, the firmware images and update bundles
we publish, and the default configuration they ship with.

Out of scope: vulnerabilities in third-party components that are already
public upstream (please report those upstream; we will still update promptly),
physical attacks requiring access to the boot device (SD card, eMMC, SSD), and issues that require an
already authenticated Admin account to exploit.
