---
layout: home

hero:
  name: Slicer Engine
  text: 3D model slicing, in Rust.
  tagline: One engine, three surfaces — CLI, WebSocket server, and Angular UI via WebAssembly. Built on Clipper2.
  actions:
    - theme: brand
      text: Get Started
      link: /guide/
    - theme: alt
      text: Architecture
      link: /architecture/scene
    - theme: alt
      text: View on GitHub
      link: https://github.com/max-scopp/slicer-engine

features:
  - title: One engine, three surfaces
    details: The same Rust core powers the CLI, the WebSocket server, and the Angular UI via WebAssembly. No drift, no second source of truth — previews and final output agree by construction.
  - title: Clipper2 under the hood
    details: Battle-tested polygon clipping for surfaces, infill boundaries, and wall offsets — wrapped in a clean Rust API and orchestrated by the slicing pipeline.
  - title: Scene engine SSOT
    details: A unified scene module owns object placement, orientation, and transforms across every surface. CLI flags and UI gestures both translate to the same SceneOp.
  - title: Modern formats & dialects
    details: STL, OBJ, and 3MF in. Marlin- and Klipper-flavored G-code out, with custom start/end blocks and lifecycle markers.
  - title: TOML configuration
    details: Layered slicer.toml — defaults, then user, then project, then CLI flags. Deep-merged, per-object overrides, validated at the boundary.
  - title: Search-first docs
    details: Every page is indexed locally. Hit `/` and start typing.
---
