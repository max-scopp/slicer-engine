---
layout: home

hero:
  name: Slicer Engine
  text: 3D model slicing, in Rust.
  tagline: A high-performance slicer engine powered by Clipper2 — native, WebAssembly, and CLI from one codebase.
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
    details: The same Rust core powers the CLI, the WebSocket server, and the Angular UI via WebAssembly. No drift, no second source of truth.
  - title: Clipper2 under the hood
    details: Battle-tested polygon clipping for surfaces, infill boundaries, and wall offsets — wrapped in a clean Rust API.
  - title: Search-first docs
    details: Every page is indexed locally. Hit `/` and start typing.
---
