# J773g (M4 Mac mini) "U1" reference tree: HDMI desktop + front USB

This is the exact kernel source that booted Aurora on the M4 Mac mini (J773g, T8132) with an HDMI desktop (SDDM + Hyprland), GPU compute (G16) and the front USB2 ports working (Nov 6-7 2026).

- Base: aurora-silicon/linux `integration/aurora` at 05ae9bf1beeb (Ryan's #1032 merge).
- On top: 19 commits. Five are clean `drm/asahi g16`, `soc: apple: pmp` and `pmdomain` patches; the rest are `TEST ONLY` bring-up commits (DCP/DPTX display, DART handoff, front USB, AQC113 RX).
- Booted source commit: 0714d188c621 (the commit just below this README commit). Kernel config: `j773g-u1/config`. Artifact hashes: `j773g-u1/SHA256SUMS`.
- The kernel build was reproduced bit for bit with aarch64 GCC 15.2.0 (Ubuntu 26.04).

Display fix chain (for anyone porting another M4): T8122 DPTX PHY sequence -> DCP firmware-uuid -> J773g DART 6-segment ADT profile -> decouple HDMI from the loader alias -> retained framebuffer.
Rules of thumb (credit: Pavel Shago / shgpavel):
- Never write the DCP power register; 0x1000030f means auto, not off.
- Echo oslog type-2 before quiesce.
- DCP DVAs need bit 40.
- Keep `ps_pmc` always-on (aurora-silicon/linux#1181), or the DCP dies on HDMI unplug.

Not working yet: USB-C / DP alt-mode display (the M4 ATC driver refuses DP alt mode), 4K60 (capped at 4K30 by the dcp_m4_ext filter).

Provenance: the G16 GPU firmware-interface details came from analysis of Apple's GPU control-coprocessor firmware (not the GPU ISA). This is a reference/test tree, not an upstream submission. The `TEST ONLY` commits are being split into proper patches for aurora-silicon/linux.

Status: work in progress and unsupported. It may break your machine; always keep a working macOS/recovery path.
