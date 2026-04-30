import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import {
  AmbientLight,
  BufferAttribute,
  BufferGeometry,
  DirectionalLight,
  GridHelper,
  Mesh,
  MeshStandardMaterial,
  Object3D,
  PerspectiveCamera,
  Scene,
  WebGLRenderer,
} from 'three';
import { SceneCommand } from '../../services/scene-command/scene-command';
import { SceneEngineService } from '../../services/scene-engine.service';

interface TrackedObject {
  id: bigint;
  name: string;
  mesh: Mesh;
}

/**
 * Minimal end-to-end demonstration of the WASM scene engine.
 *
 * Loads an STL via `SceneEngineService.addMesh`, renders the resulting
 * `RenderBuffer` with Three.js, and dispatches `SceneOp` mutations whose
 * recomputed transform stream is mirrored back onto the scene graph each
 * frame. This is the golden path: bytes -> WASM -> render -> op -> WASM ->
 * render. No drag handles, no gizmos, no business logic.
 */
@Component({
  selector: 'nexus-scene-demo',
  standalone: true,
  template: `
    <div class="layout">
      <aside class="panel">
        <h2>Scene Engine — Golden Path</h2>
        <p class="hint">
          Add an STL (or the built-in cube) to verify the WASM scene engine pipeline: bytes parsed
          in WASM, render buffer streamed to Three.js, transform ops applied in WASM, snapshot
          signals re-render the matrix.
        </p>

        <div class="block">
          <label class="file">
            <input
              type="file"
              accept=".stl,.obj"
              (change)="onFileSelected($event)"
              [disabled]="!ready()"
            />
            <span>{{ ready() ? 'Add mesh from file…' : 'Loading WASM…' }}</span>
          </label>
          <button type="button" (click)="addUnitCube()" [disabled]="!ready()">
            Add 20 mm test cube
          </button>
        </div>

        <div class="block">
          <h3>Objects ({{ objects().length }})</h3>
          @if (objects().length === 0) {
            <p class="muted">No objects yet.</p>
          } @else {
            <ul class="objects">
              @for (obj of objects(); track obj.id) {
                <li [class.selected]="obj.id === selectedId()" (click)="selectedId.set(obj.id)">
                  #{{ obj.id }} · {{ obj.name }}
                  <span class="aabb">
                    Δ {{ obj.translation[0].toFixed(1) }}, {{ obj.translation[1].toFixed(1) }},
                    {{ obj.translation[2].toFixed(1) }}
                  </span>
                </li>
              }
            </ul>
          }
        </div>

        @if (selectedObject(); as sel) {
          <div class="block">
            <h3>Ops on #{{ sel.id }}</h3>
            <div class="ops-grid">
              <button type="button" (click)="translate([10, 0, 0])">+X 10</button>
              <button type="button" (click)="translate([-10, 0, 0])">−X 10</button>
              <button type="button" (click)="translate([0, 10, 0])">+Y 10</button>
              <button type="button" (click)="translate([0, -10, 0])">−Y 10</button>
              <button type="button" (click)="translate([0, 0, 10])">+Z 10</button>
              <button type="button" (click)="translate([0, 0, -10])">−Z 10</button>
              <button type="button" (click)="rotateZ(15)">Rot Z +15°</button>
              <button type="button" (click)="rotateZ(-15)">Rot Z −15°</button>
              <button type="button" (click)="scale(1.1)">Scale ×1.1</button>
              <button type="button" (click)="scale(1 / 1.1)">Scale ÷1.1</button>
              <button type="button" (click)="centerOnBed()">Center on bed</button>
              <button type="button" (click)="dropToFloor()">Drop to floor</button>
              <button type="button" class="danger" (click)="remove()">Remove</button>
            </div>
            <pre class="snapshot">{{ formatSnapshot(sel) }}</pre>
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
      .file {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-bottom: 8px;
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
      button.danger {
        grid-column: 1 / -1;
        background: #4a2424;
        border-color: #5e2c2c;
      }
      .ops-grid {
        display: grid;
        grid-template-columns: repeat(2, 1fr);
        gap: 6px;
      }
      ul.objects {
        list-style: none;
        margin: 0;
        padding: 0;
      }
      ul.objects li {
        padding: 6px 8px;
        border-radius: 4px;
        cursor: pointer;
        display: flex;
        flex-direction: column;
      }
      ul.objects li:hover {
        background: #2a2f37;
      }
      ul.objects li.selected {
        background: #2c3a4f;
      }
      .aabb {
        font:
          11px/1.2 ui-monospace,
          monospace;
        color: #9aa4b2;
      }
      .muted {
        color: #6b7480;
        margin: 0;
      }
      .snapshot {
        margin-top: 10px;
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
export class SceneDemoComponent implements AfterViewInit, OnDestroy {
  private readonly engine = inject(SceneEngineService);
  private readonly sceneCommand = inject(SceneCommand);
  private readonly hostRef = viewChild.required<ElementRef<HTMLDivElement>>('host');

  readonly ready = signal(false);
  readonly selectedId = signal<bigint | null>(null);

  readonly objects = computed(() => this.engine.objects());
  readonly selectedObject = computed(() => {
    const id = this.selectedId();
    return id === null ? null : (this.objects().find((o) => o.id === id) ?? null);
  });

  private renderer: WebGLRenderer | null = null;
  private threeScene: Scene | null = null;
  private camera: PerspectiveCamera | null = null;
  private rafHandle = 0;
  private resizeObserver: ResizeObserver | null = null;
  private readonly tracked = new Map<bigint, TrackedObject>();

  constructor() {
    // Mirror the WASM snapshot to the Three.js scene whenever it changes.
    // Adds new meshes from the latest render buffer; drops removed ones;
    // updates matrices on the next frame from `getMatrix(id)`.
    effect(() => {
      const objs = this.objects();
      this.syncTrackedObjects(objs.map((o) => ({ id: o.id, name: o.name })));
    });

    void this.engine.ready().then(() => this.ready.set(true));
  }

  ngAfterViewInit(): void {
    this.initThree();
  }

  ngOnDestroy(): void {
    cancelAnimationFrame(this.rafHandle);
    this.resizeObserver?.disconnect();
    for (const t of this.tracked.values()) {
      t.mesh.geometry.dispose();
    }
    this.tracked.clear();
    this.renderer?.dispose();
  }

  // ---- File / sample loading -------------------------------------------------

  async onFileSelected(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) {
      return;
    }
    const ext = file.name.split('.').pop()?.toLowerCase();
    if (ext !== 'stl' && ext !== 'obj') {
      console.warn('Unsupported format', ext);
      return;
    }
    const bytes = new Uint8Array(await file.arrayBuffer());
    await this.engine.ready();
    const id = this.engine.addMesh(file.name, ext, bytes);
    this.selectedId.set(id);
    input.value = '';
  }

  async addUnitCube(): Promise<void> {
    await this.engine.ready();
    const stl = buildBinaryCubeStl(20);
    const id = this.engine.addMesh('cube-20mm.stl', 'stl', stl);
    this.selectedId.set(id);
  }

  // ---- Op dispatchers --------------------------------------------------------

  private withSelected(fn: (id: bigint) => void): void {
    const id = this.selectedId();
    if (id !== null) {
      fn(id);
    }
  }

  translate(delta: [number, number, number]): void {
    this.withSelected((id) => this.sceneCommand.apply({ op: 'translate', args: { id, delta } }));
  }
  rotateZ(degrees: number): void {
    this.withSelected((id) =>
      this.sceneCommand.apply({ op: 'rotate', args: { id, axis: [0, 0, 1], degrees } }),
    );
  }
  scale(factor: number): void {
    this.withSelected((id) =>
      this.sceneCommand.apply({
        op: 'scale',
        args: { id, factors: [factor, factor, factor] },
      }),
    );
  }
  centerOnBed(): void {
    this.withSelected((id) => this.sceneCommand.apply({ op: 'center_on_bed', args: { id } }));
  }
  dropToFloor(): void {
    this.withSelected((id) => this.sceneCommand.apply({ op: 'drop_to_floor', args: { id } }));
  }
  remove(): void {
    this.withSelected((id) => {
      this.sceneCommand.apply({ op: 'remove', args: { id } });
      this.selectedId.set(null);
    });
  }

  // ---- Three.js -------------------------------------------------------------

  private initThree(): void {
    const host = this.hostRef().nativeElement;
    const renderer = new WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    host.appendChild(renderer.domElement);

    const scene = new Scene();
    const bed = this.engine.bed();
    const camera = new PerspectiveCamera(45, 1, 1, 5000);
    // Z-up bed: place camera off the +X/+Y/+Z corner looking at the bed centre.
    const cx = bed.width / 2;
    const cy = bed.depth / 2;
    camera.up.set(0, 0, 1);
    camera.position.set(cx + 250, cy + 250, 200);
    camera.lookAt(cx, cy, 0);

    scene.add(new AmbientLight(0xffffff, 0.5));
    const dir = new DirectionalLight(0xffffff, 0.8);
    dir.position.set(200, 200, 400);
    scene.add(dir);

    // Bed grid in the XY plane, centred on the bed.
    const grid = new GridHelper(Math.max(bed.width, bed.depth), 20, 0x4a5160, 0x2c3036);
    grid.rotation.x = Math.PI / 2;
    grid.position.set(cx, cy, 0);
    scene.add(grid);

    this.renderer = renderer;
    this.threeScene = scene;
    this.camera = camera;

    this.resizeObserver = new ResizeObserver(() => this.handleResize());
    this.resizeObserver.observe(host);
    this.handleResize();

    const tick = () => {
      this.updateMatrices();
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

  private syncTrackedObjects(latest: { id: bigint; name: string }[]): void {
    const seen = new Set<bigint>();
    for (const { id, name } of latest) {
      seen.add(id);
      if (!this.tracked.has(id)) {
        this.addTrackedObject(id, name);
      }
    }
    for (const id of [...this.tracked.keys()]) {
      if (!seen.has(id)) {
        const t = this.tracked.get(id)!;
        this.threeScene?.remove(t.mesh);
        t.mesh.geometry.dispose();
        this.tracked.delete(id);
      }
    }
  }

  private addTrackedObject(id: bigint, name: string): void {
    if (!this.threeScene) {
      return;
    }
    const buf = this.engine.getRenderBuffer(id);
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(buf.positions, 3));
    geometry.setAttribute('normal', new BufferAttribute(buf.normals, 3));
    geometry.setIndex(new BufferAttribute(buf.indices, 1));
    const material = new MeshStandardMaterial({
      color: 0x4a90e2,
      metalness: 0.1,
      roughness: 0.6,
      flatShading: true,
    });
    const mesh = new Mesh(geometry, material);
    mesh.name = name;
    mesh.matrixAutoUpdate = false;
    this.threeScene.add(mesh);
    this.tracked.set(id, { id, name, mesh });
    this.applyMatrix(id, mesh);
  }

  private updateMatrices(): void {
    // Snapshot ids the WASM engine still knows about. The signal effect
    // that prunes `this.tracked` may not have run yet (between the
    // synchronous Remove op and the RAF tick), so we filter against the
    // authoritative snapshot to avoid `getMatrix` throwing for stale ids.
    const liveIds = new Set(this.objects().map((o) => o.id));
    for (const { id, mesh } of this.tracked.values()) {
      if (!liveIds.has(id)) {
        continue;
      }
      this.applyMatrix(id, mesh);
    }
  }

  private applyMatrix(id: bigint, mesh: Object3D): void {
    const m = this.engine.getMatrix(id);
    // Three's `Matrix4.fromArray` reads column-major, matching glam's layout.
    mesh.matrix.fromArray(m);
    mesh.matrixWorldNeedsUpdate = true;
  }

  formatSnapshot(obj: ReturnType<SceneEngineService['snapshot']>['objects'][number]): string {
    const t = obj.translation;
    const r = obj.euler_xyz_deg;
    const s = obj.scale;
    const a = obj.world_aabb;
    return [
      `translation : ${t[0].toFixed(2)}, ${t[1].toFixed(2)}, ${t[2].toFixed(2)}`,
      `rotation°   : ${r[0].toFixed(1)}, ${r[1].toFixed(1)}, ${r[2].toFixed(1)}`,
      `scale       : ${s[0].toFixed(3)}, ${s[1].toFixed(3)}, ${s[2].toFixed(3)}`,
      `world AABB  : (${a[0].map((v) => v.toFixed(1)).join(', ')})`,
      `              (${a[1].map((v) => v.toFixed(1)).join(', ')})`,
      `triangles   : ${obj.triangle_count}`,
    ].join('\n');
  }
}

/**
 * Build a tiny binary STL containing a single axis-aligned cube of the given
 * edge length, centred on the origin. Used for the "Add test cube" button so
 * the demo works without a file picker.
 */
function buildBinaryCubeStl(edge: number): Uint8Array {
  const h = edge / 2;
  // 12 triangles, each: normal (3 f32) + 3 verts (9 f32) + attrib (u16) = 50 bytes.
  const HEADER = 80;
  const TRI_BYTES = 50;
  const TRI_COUNT = 12;
  const buf = new ArrayBuffer(HEADER + 4 + TRI_BYTES * TRI_COUNT);
  const view = new DataView(buf);
  view.setUint32(HEADER, TRI_COUNT, true);

  type Tri = [
    [number, number, number], // normal
    [number, number, number],
    [number, number, number],
    [number, number, number],
  ];
  const tris: Tri[] = [
    // -Z bottom
    [
      [0, 0, -1],
      [-h, -h, -h],
      [h, -h, -h],
      [h, h, -h],
    ],
    [
      [0, 0, -1],
      [-h, -h, -h],
      [h, h, -h],
      [-h, h, -h],
    ],
    // +Z top
    [
      [0, 0, 1],
      [-h, -h, h],
      [h, h, h],
      [h, -h, h],
    ],
    [
      [0, 0, 1],
      [-h, -h, h],
      [-h, h, h],
      [h, h, h],
    ],
    // -Y front
    [
      [0, -1, 0],
      [-h, -h, -h],
      [-h, -h, h],
      [h, -h, h],
    ],
    [
      [0, -1, 0],
      [-h, -h, -h],
      [h, -h, h],
      [h, -h, -h],
    ],
    // +Y back
    [
      [0, 1, 0],
      [-h, h, -h],
      [h, h, h],
      [-h, h, h],
    ],
    [
      [0, 1, 0],
      [-h, h, -h],
      [h, h, -h],
      [h, h, h],
    ],
    // -X left
    [
      [-1, 0, 0],
      [-h, -h, -h],
      [-h, h, -h],
      [-h, h, h],
    ],
    [
      [-1, 0, 0],
      [-h, -h, -h],
      [-h, h, h],
      [-h, -h, h],
    ],
    // +X right
    [
      [1, 0, 0],
      [h, -h, -h],
      [h, h, h],
      [h, h, -h],
    ],
    [
      [1, 0, 0],
      [h, -h, -h],
      [h, -h, h],
      [h, h, h],
    ],
  ];

  let offset = HEADER + 4;
  for (const [n, v1, v2, v3] of tris) {
    for (const c of n) {
      view.setFloat32(offset, c, true);
      offset += 4;
    }
    for (const v of [v1, v2, v3]) {
      for (const c of v) {
        view.setFloat32(offset, c, true);
        offset += 4;
      }
    }
    view.setUint16(offset, 0, true);
    offset += 2;
  }
  return new Uint8Array(buf);
}
