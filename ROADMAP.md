## SARPRO Roadmap

- Last updated: 2025-08-16
- Changelog: `CHANGELOG.md`

### Vision
Deliver production‑grade ARD for Sentinel‑1 GRD with extreme throughput and great DX: COG/STAC‑native outputs, RTC, tiling, and turnkey change products.

### Tracks and phases (0.x)
- Phase 1: Foundations, options plumbing, remote I/O (VSICURL), STAC‑in
- Phase 2: COG + overviews, STAC‑out, QC JSON sidecar, official Docker image
- Phase 3: Baseline quality/masking (border, NESZ, incidence)
- Phase 4: Speckle filters (Lee, Refined Lee, Kuan, Frost)
- Phase 5: DEM‑based RTC + angular normalization, layover/shadow approx
- Phase 6: Tiling/Chipping (WebMercator/UTM grids), per‑tile STAC
- Phase 7: Time‑series stack + change (log‑ratio, n‑diff, composites)
- Phase 8: Local tile server + visualization presets

### Release targets (tentative)
- 0.3.0: Phases 1–2
- 0.4.0: Phases 3–4
- 0.5.x: Phase 5
- 0.6.x: Phase 6–7
- 0.7.x: Phase 8

### Public status
- Active development; API is evolving pre‑1.0. See `CHANGELOG.md`.

### How to follow
- Watch releases on GitHub; open issues with sample product IDs and expected outputs.


