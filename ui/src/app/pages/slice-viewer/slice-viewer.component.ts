import { Component, input } from '@angular/core';

@Component({
  selector: 'nexus-slice-viewer',
  standalone: true,
  imports: [],
  templateUrl: './slice-viewer.component.html',
  styleUrl: './slice-viewer.component.scss',
})
export class SliceViewerComponent {
  readonly requestUuid = input<string>('');
  readonly printEstimates = input({
    status: 'modified' as const,
    time: '3h 12m',
    material: '42g',
    materialDelta: '-14.2m',
    cost: '$1.15',
    layers: 320,
  });
}
