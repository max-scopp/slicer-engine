import { DecimalPipe } from '@angular/common';
import {
    AfterViewInit,
    ChangeDetectionStrategy,
    Component,
    ElementRef,
    OnDestroy,
    computed,
    signal,
    viewChild,
} from '@angular/core';
import {
    AmbientLight,
    BufferAttribute,
    BufferGeometry,
    DirectionalLight,
    GridHelper,
    Group,
    LineBasicMaterial,
    LineSegments,
    PerspectiveCamera,
    Scene,
    WebGLRenderer,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import init, {
    GcodeHandle,
    type GcodeLayerBuffer,
} from '../../../generated/scene-wasm/scene_engine';

// ── Role colour palette ──────────────────────────────────────────────────────

const ROLE_COLORS = {
  outerWall: 0xff8800,
  innerWall: 0xffcc00,
  infill: 0xcc44ff,
  topSurface: 0xff3355,
  bottomSurface: 0x00bbff,
  travel: 0x334466,
  other: 0x44ffaa,
} as const;

type RoleName = keyof typeof ROLE_COLORS;

const ROLE_LABELS: Record<RoleName, string> = {
  outerWall: 'Outer Wall',
  innerWall: 'Inner Wall',
  infill: 'Infill',
  topSurface: 'Top Surface',
  bottomSurface: 'Bottom Surface',
  travel: 'Travel',
  other: 'Other',
};

const ROLE_CSS: Record<RoleName, string> = {
  outerWall: '#ff8800',
  innerWall: '#ffcc00',
  infill: '#cc44ff',
  topSurface: '#ff3355',
  bottomSurface: '#00bbff',
  travel: '#334466',
  other: '#44ffaa',
};

// ── Layer metadata ───────────────────────────────────────────────────────────

interface RoleSegments {
  role: RoleName;
  lines: LineSegments;
  /** Number of line segments (positions.length / 6). */
  count: number;
}

interface LayerInfo {
  index: number;
  z: number;
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
}

/**
 * GCode demo page — Rust/WASM parses the file, Three.js renders the result.
 *
 * Mirrors the scene-demo methodology: all file reading and understanding
 * happens inside the WASM module (`GcodeHandle.parse`). The resulting
 * per-layer, per-role `Float32Array` buffers are handed directly to Three.js
 * `LineSegments`. No GCode parsing occurs in JavaScript.
 */
@Component({
  selector: 'nexus-gcode-demo',
  standalone: true,
  imports: [DecimalPipe],
  template: `
    <div class="layout">
      <aside class="panel">
        <h2>GCode Viewer — Demo</h2>
        <p class="hint">
          GCode is parsed entirely in Rust/WASM. The resulting per-layer geometry buffers are sent
          directly to Three.js for rendering. No JavaScript parsing.
        </p>

        <div class="block">
          <label class="file-label">
            <input
              type="file"
              accept=".gcode,.gco,.g"
              (change)="onFileSelected($event)"
              [disabled]="loading()"
            />
            <span>{{ loading() ? 'Parsing…' : 'Load .gcode file…' }}</span>
          </label>
        </div>

        @if (layerCount() > 0) {
          <div class="block">
            <h3>Layers ({{ layerCount() }} total)</h3>
            <p class="layer-z">
              Layers {{ layerMin() + 1 }}–{{ layerMax() + 1 }} / {{ layerCount() }} &nbsp;·&nbsp; Z
              {{ minZ() | number: '1.3-3' }}–{{ currentZ() | number: '1.3-3' }} mm
            </p>
            <div class="range-slider">
              <input
                type="range"
                class="thumb thumb-min"
                min="0"
                [max]="layerCount() - 1"
                [value]="layerMin()"
                (input)="onLayerMinInput($event)"
              />
              <input
                type="range"
                class="thumb thumb-max"
                min="0"
                [max]="layerCount() - 1"
                [value]="layerMax()"
                (input)="onLayerMaxInput($event)"
              />
              <div
                class="range-track-fill"
                [style.left.%]="(layerMin() / (layerCount() - 1)) * 100"
                [style.right.%]="((layerCount() - 1 - layerMax()) / (layerCount() - 1)) * 100"
              ></div>
            </div>
            <div class="layer-buttons">
              <button type="button" (click)="stepLayerMin(-1)" [disabled]="layerMin() === 0">
                ◀ Min −
              </button>
              <button type="button" (click)="stepLayerMin(1)" [disabled]="layerMin() >= layerMax()">
                Min + ▶
              </button>
              <button
                type="button"
                (click)="stepLayerMax(-1)"
                [disabled]="layerMax() <= layerMin()"
              >
                ◀ Max −
              </button>
              <button
                type="button"
                (click)="stepLayerMax(1)"
                [disabled]="layerMax() === layerCount() - 1"
              >
                Max + ▶
              </button>
            </div>
          </div>

          <div class="block">
            <h3>Layer Progress</h3>
            <p class="layer-z">Move {{ segmentProgress() }} / {{ currentLayerTotalSegments() }}</p>
            <input
              type="range"
              class="slider"
              min="0"
              [max]="currentLayerTotalSegments()"
              [value]="segmentProgress()"
              (input)="onSegmentSliderInput($event)"
            />
          </div>

          <div class="block">
            <h3>Legend</h3>
            <ul class="legend">
              @for (entry of legendEntries; track entry.role) {
                <li
                  [class.role-hidden]="hiddenRoles().has(entry.role)"
                  (click)="toggleRole(entry.role)"
                >
                  <span class="swatch" [style.background]="entry.css"></span>
                  {{ entry.label }}
                </li>
              }
            </ul>
          </div>

          <div class="block">
            <h3>Stats</h3>
            <pre class="stats">{{ statsText() }}</pre>
          </div>
        }
      </aside>

      <div #host class="viewer"></div>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        width: 100%;
        height: 100vh;
        overflow: hidden;
        background: #1a1d22;
        color: #e6e6e6;
        font:
          13px/1.4 ui-sans-serif,
          system-ui;
      }
      .layout {
        display: grid;
        grid-template-columns: 340px 1fr;
        height: 100%;
      }
      .panel {
        padding: 16px;
        border-right: 1px solid #2c3036;
        overflow-y: auto;
        background: #20242a;
      }
      .panel h2 {
        margin: 0 0 8px;
        font-size: 16px;
      }
      .panel h3 {
        margin: 0 0 8px;
        font-size: 13px;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: #9aa4b2;
      }
      .hint {
        color: #9aa4b2;
        margin: 0 0 16px;
      }
      .block {
        margin-bottom: 20px;
      }
      .file-label {
        display: flex;
        align-items: center;
        gap: 8px;
        cursor: pointer;
      }
      .file-label span {
        padding: 6px 10px;
        background: #2f343c;
        border: 1px solid #3b4048;
        border-radius: 4px;
      }
      .file-label span:hover {
        background: #3a414b;
      }
      button {
        padding: 6px 10px;
        background: #2f343c;
        color: inherit;
        border: 1px solid #3b4048;
        border-radius: 4px;
        cursor: pointer;
        font: inherit;
      }
      button:hover:not(:disabled) {
        background: #3a414b;
      }
      button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      .layer-z {
        margin: 0 0 8px;
        font:
          11px/1.4 ui-monospace,
          monospace;
        color: #c8cdd4;
      }
      /* ── Dual-thumb range slider ───────────────────────── */
      .range-slider {
        position: relative;
        height: 20px;
        margin-bottom: 8px;
      }
      .range-slider .thumb {
        position: absolute;
        width: 100%;
        height: 4px;
        top: 8px;
        pointer-events: none;
        appearance: none;
        -webkit-appearance: none;
        background: transparent;
        outline: none;
      }
      .range-slider .thumb::-webkit-slider-thumb {
        appearance: none;
        -webkit-appearance: none;
        pointer-events: all;
        width: 14px;
        height: 14px;
        border-radius: 50%;
        background: #7aa4d4;
        border: 2px solid #1a1d22;
        cursor: pointer;
      }
      .range-slider .thumb::-moz-range-thumb {
        pointer-events: all;
        width: 14px;
        height: 14px;
        border-radius: 50%;
        background: #7aa4d4;
        border: 2px solid #1a1d22;
        cursor: pointer;
      }
      .range-slider .thumb-min::-webkit-slider-thumb {
        background: #a0c4e8;
      }
      .range-slider .thumb-min::-moz-range-thumb {
        background: #a0c4e8;
      }
      .range-track-fill {
        position: absolute;
        top: 10px;
        height: 4px;
        background: #4a7aaa;
        border-radius: 2px;
        pointer-events: none;
      }
      /* base track rendered behind both thumbs */
      .range-slider::before {
        content: '';
        position: absolute;
        top: 10px;
        left: 0;
        right: 0;
        height: 4px;
        background: #2c3036;
        border-radius: 2px;
      }
      /* ───────────────────────────────────────────────────── */
      .layer-buttons {
        display: flex;
        gap: 6px;
        flex-wrap: wrap;
      }
      .layer-buttons button {
        flex: 1;
        min-width: 60px;
      }
      ul.legend {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 4px;
      }
      ul.legend li {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 12px;
        cursor: pointer;
        padding: 3px 4px;
        border-radius: 3px;
        user-select: none;
      }
      ul.legend li:hover {
        background: #2a2f37;
      }
      ul.legend li.role-hidden {
        opacity: 0.35;
        text-decoration: line-through;
      }
      ul.legend li.role-hidden .swatch {
        opacity: 0.3;
      }
      .swatch {
        display: inline-block;
        width: 14px;
        height: 14px;
        border-radius: 2px;
        flex-shrink: 0;
      }
      .stats {
        margin: 0;
        padding: 8px;
        background: #161a1f;
        border-radius: 4px;
        font:
          11px/1.4 ui-monospace,
          monospace;
        color: #c8cdd4;
        white-space: pre-wrap;
      }
      .viewer {
        position: relative;
        background: #0f1115;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class GcodeDemoComponent implements AfterViewInit, OnDestroy {
  private readonly hostRef = viewChild.required<ElementRef<HTMLDivElement>>('host');

  // ── Reactive state ──────────────────────────────────────────────────────

  readonly loading = signal(false);
  readonly layerMin = signal(0);
  readonly layerMax = signal(0);
  readonly segmentProgress = signal(0);
  readonly hiddenRoles = signal<ReadonlySet<RoleName>>(new Set<RoleName>());

  private readonly layers = signal<LayerInfo[]>([]);
  private readonly currentLayerBuffer = signal<GcodeLayerBuffer | null>(null);

  readonly layerCount = computed(() => this.layers().length);
  readonly currentLayerTotalSegments = computed(() => {
    const infos = this.layers();
    const idx = this.layerMax();
    return infos[idx]?.totalSegments ?? 0;
  });
  readonly currentZ = computed(() => {
    const infos = this.layers();
    const idx = this.layerMax();
    return infos[idx]?.z ?? 0;
  });
  readonly minZ = computed(() => {
    const infos = this.layers();
    const idx = this.layerMin();
    return infos[idx]?.z ?? 0;
  });

  readonly statsText = computed(() => {
    const buf = this.currentLayerBuffer();
    if (!buf) {
      return '';
    }
    const seg = (arr: Float32Array) => arr.length / 6;
    return [
      `outer wall   : ${seg(buf.outer_wall)}`,
      `inner wall   : ${seg(buf.inner_wall)}`,
      `infill       : ${seg(buf.infill)}`,
      `top surface  : ${seg(buf.top_surface)}`,
      `bottom surf. : ${seg(buf.bottom_surface)}`,
      `travel       : ${seg(buf.travel)}`,
      `other        : ${seg(buf.other)}`,
    ].join('\n');
  });

  readonly legendEntries = (Object.keys(ROLE_COLORS) as RoleName[]).map((role) => ({
    role,
    label: ROLE_LABELS[role],
    css: ROLE_CSS[role],
  }));

  // ── Three.js state ───────────────────────────────────────────────────────

  private renderer: WebGLRenderer | null = null;
  private threeScene: Scene | null = null;
  private camera: PerspectiveCamera | null = null;
  private controls: OrbitControls | null = null;
  private rafHandle = 0;
  private resizeObserver: ResizeObserver | null = null;

  // ── Lifecycle ────────────────────────────────────────────────────────────

  ngAfterViewInit(): void {
    this.initThree();
  }

  ngOnDestroy(): void {
    cancelAnimationFrame(this.rafHandle);
    this.resizeObserver?.disconnect();
    this.controls?.dispose();
    this.disposeLayers();
    this.renderer?.dispose();
  }

  // ── File loading ─────────────────────────────────────────────────────────

  async onFileSelected(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) {
      return;
    }

    this.loading.set(true);
    try {
      await init({ module_or_path: 'scene_engine_bg.wasm' });
      const bytes = new Uint8Array(await file.arrayBuffer());
      const handle = GcodeHandle.parse(bytes);
      this.buildScene(handle);
    } finally {
      this.loading.set(false);
    }
  }

  // ── Layer controls ────────────────────────────────────────────────────────

  onLayerMinInput(event: Event): void {
    const raw = parseInt((event.target as HTMLInputElement).value, 10);
    const clamped = Math.min(raw, this.layerMax());
    this.showLayerRange(clamped, this.layerMax());
  }

  onLayerMaxInput(event: Event): void {
    const raw = parseInt((event.target as HTMLInputElement).value, 10);
    const clamped = Math.max(raw, this.layerMin());
    this.showLayerRange(this.layerMin(), clamped);
  }

  onSegmentSliderInput(event: Event): void {
    const value = parseInt((event.target as HTMLInputElement).value, 10);
    this.applySegmentProgress(value);
  }

  stepLayerMin(delta: number): void {
    const next = Math.max(0, Math.min(this.layerMax(), this.layerMin() + delta));
    this.showLayerRange(next, this.layerMax());
  }

  stepLayerMax(delta: number): void {
    const next = Math.max(
      this.layerMin(),
      Math.min(this.layerCount() - 1, this.layerMax() + delta),
    );
    this.showLayerRange(this.layerMin(), next);
  }

  toggleRole(role: RoleName): void {
    const current = this.hiddenRoles();
    const next = new Set(current);
    if (next.has(role)) {
      next.delete(role);
    } else {
      next.add(role);
    }
    this.hiddenRoles.set(next);
    const visible = !next.has(role);
    for (const info of this.layers()) {
      for (const rs of info.roleSegments) {
        if (rs.role === role) {
          rs.lines.visible = visible;
        }
      }
    }
  }

  // ── Scene building ────────────────────────────────────────────────────────

  private buildScene(handle: GcodeHandle): void {
    this.disposeLayers();

    const count = handle.layerCount();
    const infos: LayerInfo[] = [];

    for (let i = 0; i < count; i++) {
      const buf = handle.getLayer(i);
      const { group, totalSegments, roleSegments } = buildLayerGroup(buf);
      group.visible = false;
      this.threeScene?.add(group);
      infos.push({ index: i, z: buf.z, group, totalSegments, roleSegments });
    }

    this.layers.set(infos);
    this.showLayerRange(0, count - 1);
  }

  private showLayerRange(min: number, max: number): void {
    const infos = this.layers();

    // Restore the previous top layer's draw range before potentially changing it.
    const prevMax = this.layerMax();
    const prevInfo = infos[prevMax];
    if (prevInfo && prevMax !== max) {
      for (const { lines } of prevInfo.roleSegments) {
        lines.geometry.setDrawRange(0, Infinity);
      }
    }

    for (const info of infos) {
      info.group.visible = info.index >= min && info.index <= max;
    }
    this.layerMin.set(min);
    this.layerMax.set(max);

    const info = infos[max];
    this.currentLayerBuffer.set(
      (info?.group.userData['handle'] as GcodeLayerBuffer | undefined) ?? null,
    );
    // Reset progress to show the full top layer.
    this.applySegmentProgress(info?.totalSegments ?? 0);
  }

  /**
   * Scrub through the segments of the current layer up to `p`.
   *
   * Roles are filled in display order (outer wall → inner wall → infill →
   * top/bottom surface → travel → other). Each role's `LineSegments` geometry
   * gets a `drawRange` that reveals exactly as many segments as the progress
   * value allocates to it. Layers below the current are always fully visible.
   */
  private applySegmentProgress(p: number): void {
    const infos = this.layers();
    const info = infos[this.layerMax()];
    if (!info) {
      return;
    }
    this.segmentProgress.set(p);
    let remaining = p;
    for (const { lines, count } of info.roleSegments) {
      const show = Math.min(remaining, count);
      // LineSegments drawRange.count is in vertices: 2 vertices per segment.
      lines.geometry.setDrawRange(0, show * 2);
      remaining = Math.max(0, remaining - count);
    }
  }

  private disposeLayers(): void {
    const infos = this.layers();
    for (const info of infos) {
      disposeGroup(info.group);
      this.threeScene?.remove(info.group);
    }
    this.layers.set([]);
    this.currentLayerBuffer.set(null);
    this.segmentProgress.set(0);
    this.layerMin.set(0);
    this.layerMax.set(0);
    this.hiddenRoles.set(new Set<RoleName>());
  }

  // ── Three.js init ─────────────────────────────────────────────────────────

  private initThree(): void {
    const host = this.hostRef().nativeElement;
    const renderer = new WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    host.appendChild(renderer.domElement);

    const scene = new Scene();

    // Default bed size — same as scene-demo defaults (220×220 mm)
    const BED_W = 220;
    const BED_D = 220;
    const cx = BED_W / 2;
    const cy = BED_D / 2;

    const camera = new PerspectiveCamera(45, 1, 0.1, 5000);
    camera.up.set(0, 0, 1);
    camera.position.set(cx + 250, cy + 250, 200);
    camera.lookAt(cx, cy, 0);

    scene.add(new AmbientLight(0xffffff, 0.6));
    const dir = new DirectionalLight(0xffffff, 0.8);
    dir.position.set(200, 200, 400);
    scene.add(dir);

    const grid = new GridHelper(Math.max(BED_W, BED_D), 20, 0x4a5160, 0x2c3036);
    grid.rotation.x = Math.PI / 2;
    grid.position.set(cx, cy, 0);
    scene.add(grid);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.1;
    controls.target.set(cx, cy, 0);
    controls.update();

    this.renderer = renderer;
    this.threeScene = scene;
    this.camera = camera;
    this.controls = controls;

    this.resizeObserver = new ResizeObserver(() => this.handleResize());
    this.resizeObserver.observe(host);
    this.handleResize();

    const tick = () => {
      controls.update();
      renderer.render(scene, camera);
      this.rafHandle = requestAnimationFrame(tick);
    };
    this.rafHandle = requestAnimationFrame(tick);
  }

  private handleResize(): void {
    const host = this.hostRef().nativeElement;
    const { clientWidth: w, clientHeight: h } = host;
    if (!this.renderer || !this.camera || w === 0 || h === 0) {
      return;
    }
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

interface LayerBuild {
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
}

/**
 * Build a `THREE.Group` containing one `LineSegments` per non-empty role
 * buffer from the given layer, in a defined display order.
 *
 * The display order determines how segment-progress scrubbing fills the layer:
 * outer walls are revealed first, then inner walls, infill, surfaces, travel.
 */
function buildLayerGroup(buf: GcodeLayerBuffer): LayerBuild {
  const group = new Group();

  // Store the buffer reference on the group for stats display.
  group.userData['handle'] = buf;

  const ROLE_ORDER: Array<{ role: RoleName; data: Float32Array; color: number }> = [
    { role: 'outerWall', data: buf.outer_wall, color: ROLE_COLORS.outerWall },
    { role: 'innerWall', data: buf.inner_wall, color: ROLE_COLORS.innerWall },
    { role: 'infill', data: buf.infill, color: ROLE_COLORS.infill },
    { role: 'topSurface', data: buf.top_surface, color: ROLE_COLORS.topSurface },
    { role: 'bottomSurface', data: buf.bottom_surface, color: ROLE_COLORS.bottomSurface },
    { role: 'travel', data: buf.travel, color: ROLE_COLORS.travel },
    { role: 'other', data: buf.other, color: ROLE_COLORS.other },
  ];

  const roleSegments: RoleSegments[] = [];
  let totalSegments = 0;

  for (const { role, data, color } of ROLE_ORDER) {
    if (data.length === 0) {
      continue;
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(data, 3));
    const material = new LineBasicMaterial({ color });
    const lines = new LineSegments(geometry, material);
    group.add(lines);
    const count = data.length / 6;
    roleSegments.push({ role, lines, count });
    totalSegments += count;
  }

  return { group, totalSegments, roleSegments };
}

/** Dispose all geometries and materials inside a group. */
function disposeGroup(group: Group): void {
  for (const child of group.children) {
    if (child instanceof LineSegments) {
      child.geometry.dispose();
      if (Array.isArray(child.material)) {
        for (const m of child.material) {
          m.dispose();
        }
      } else {
        child.material.dispose();
      }
    }
  }
}
