# Cygnus security policy

Cygnus takes security seriously, including as an open-source
creative suite. This document describes how to report a vulnerability.

## Reporting a vulnerability

**Do not open a public issue to report a vulnerability** — prefer GitHub:

- Open a private *security advisory*:
  https://github.com/vabyz971/Cygnus/security/advisories
- Or write to the maintainer via GitHub (discussion issues are
  accepted for non-sensitive cases).

We acknowledge receipt within 72 h and prioritize verified reports.
Please do not publish exploitable details before a fix is available.

## Surfaces considered sensitive

- Image decoding (PNG/JPEG) at open and export — depends on
  `image` and its decoders.
- Parsing of the `.cygp` project format (versioned JSON + base64) in
  `photo-engine` (`project.rs`).
- GPU rendering (wgpu shaders) — any uncontrolled GPU memory usage
  there is reported as a vulnerability.

## Supported versions

There is no formal release cycle yet: security fixes are published
on the `main` branch as soon as they are validated.
