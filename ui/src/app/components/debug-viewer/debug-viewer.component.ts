import { Component, inject, signal, computed, effect } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { HttpClient } from '@angular/common/http';
import { SlicerService } from '../../services/slicer.service';

export interface SerializableLayer {
  z: number;
  paths: [number, number][][];
  path_roles: string[];
}

export interface RoleToggle {
  role: string;
  enabled: boolean;
  color: string;
}

@Component({
  selector: 'app-debug-viewer',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './debug-viewer.component.html',
  styleUrl: './debug-viewer.component.scss',
})
export class DebugViewerComponent {
  private readonly http = inject(HttpClient);
  private readonly slicer = inject(SlicerService);

  readonly layers = signal<SerializableLayer[]>([]);
  readonly currentLayerIndex = signal<number>(0);
  readonly loading = signal<boolean>(false);
  readonly error = signal<string | null>(null);

  readonly roleToggles = signal<RoleToggle[]>([
    { role: 'Perimeter', enabled: true, color: '#3b82f6' },
    { role: 'TopSurface', enabled: true, color: '#ef4444' },
    { role: 'BottomSurface', enabled: true, color: '#22c55e' },
    { role: 'Infill', enabled: true, color: '#eab308' },
    { role: 'Support', enabled: true, color: '#f97316' },
    { role: 'Bridge', enabled: true, color: '#a855f7' },
    { role: 'Skirt', enabled: true, color: '#6b7280' },
  ]);

  readonly currentLayer = computed(() => {
    const layers = this.layers();
    const idx = this.currentLayerIndex();
    return layers[idx] || null;
  });

  readonly viewBox = computed(() => {
    const layers = this.layers();
    if (layers.length === 0) return '0 0 200 200';

    // Calculate bounds across all layers
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;

    for (const layer of layers) {
      for (const path of layer.paths) {
        for (const [x, y] of path) {
          minX = Math.min(minX, x);
          minY = Math.min(minY, y);
          maxX = Math.max(maxX, x);
          maxY = Math.max(maxY, y);
        }
      }
    }

    // Add padding (10% of the size)
    const width = maxX - minX;
    const height = maxY - minY;
    const padding = Math.max(width, height) * 0.1;

    return `${minX - padding} ${minY - padding} ${width + 2 * padding} ${height + 2 * padding}`;
  });

  constructor() {
    // Auto-load debug data when slicing completes
    effect(() => {
      const status = this.slicer.status();
      if (status === 'done') {
        // Extract UUID from the last previous session (most recent)
        const sessions = this.slicer.previousSessions();
        if (sessions.length > 0) {
          const mostRecent = sessions[0];
          this.loadDebugData(mostRecent.request_uuid);
        }
      }
    });

    // Listen for manual debug load requests from history panel
    effect(() => {
      const requestUuid = this.slicer.debugLoadRequest();
      if (requestUuid) {
        this.loadDebugData(requestUuid);
      }
    });
  }

  async loadDebugData(requestUuid: string): Promise<void> {
    this.loading.set(true);
    this.error.set(null);

    try {
      const data = await this.http
        .get<SerializableLayer[]>(`/api/debug/${requestUuid}`)
        .toPromise();

      if (data) {
        this.layers.set(data);
        this.currentLayerIndex.set(0);
      }
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load debug data');
    } finally {
      this.loading.set(false);
    }
  }

  toggleRole(role: string): void {
    this.roleToggles.update(toggles =>
      toggles.map(t => (t.role === role ? { ...t, enabled: !t.enabled } : t))
    );
  }

  getRoleColor(role: string): string {
    const toggle = this.roleToggles().find(t => t.role === role);
    return toggle?.color || '#888888';
  }

  isRoleEnabled(role: string): boolean {
    const toggle = this.roleToggles().find(t => t.role === role);
    return toggle?.enabled ?? true;
  }

  getPathsForLayer(layer: SerializableLayer): { path: [number, number][]; role: string }[] {
    if (!layer) return [];

    return layer.paths
      .map((path, idx) => ({
        path,
        role: layer.path_roles[idx] || 'Perimeter',
      }))
      .filter(item => this.isRoleEnabled(item.role));
  }

  pathToSvgString(path: [number, number][]): string {
    if (path.length === 0) return '';
    
    const [x0, y0] = path[0];
    let d = `M ${x0} ${y0}`;
    
    for (let i = 1; i < path.length; i++) {
      const [x, y] = path[i];
      d += ` L ${x} ${y}`;
    }
    
    // Close the path
    d += ' Z';
    
    return d;
  }
}
