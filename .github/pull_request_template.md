## Summary

<!-- What does this change and why? -->

Closes #

## Checklist

- [ ] PR title follows Conventional Commits with a scope, e.g. `feat(cec): …` (it becomes the squash commit message)
- [ ] All commits are signed off (`git commit -s`)
- [ ] Linked to an accepted issue (not needed for small fixes, docs or typos)
- [ ] Tests added or updated; they run without hardware
- [ ] `just check` passes locally
- [ ] `CHANGELOG.md` updated under `[Unreleased]`, or this is internal-only (`skip-changelog`)
- [ ] Related requirement IDs listed below, and `SPEC.md` updated if behaviour changes
- [ ] New source files have the SPDX header
- [ ] Substantial AI assistance is disclosed (`Assisted-by:` trailer or note below)

**Requirement IDs:** <!-- e.g. SYNC-007, CEC-003 -->

## Hardware testing

<!-- Required if this touches boot, DRM/KMS, CEC, Wi-Fi/AP, storage/power-loss or updates.
     Describe what you tested on real hardware, or write "not tested on hardware".
     A maintainer will add `needs-hil` and run the hardware-in-the-loop suite. -->

## Notes for reviewers

<!-- Anything that needs special attention, follow-ups, or screenshots of UI changes. -->
