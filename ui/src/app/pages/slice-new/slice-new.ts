import { Component, computed, inject } from '@angular/core';
import { Router } from '@angular/router';
import { Slicer } from '../../services/slicer';
import { SlicerFile } from '../../services/slicer-file';
import { Icon } from '../../shared/icon/icon';

@Component({
  selector: 'nexus-slice-new',
  imports: [Icon],
  templateUrl: './slice-new.component.html',
  styleUrl: './slice-new.component.scss',
})
export class SliceNew {
  private readonly router = inject(Router);
  private readonly slicer = inject(Slicer);
  readonly slicerFile = inject(SlicerFile);
  readonly uploading = computed(() => {
    const p = this.slicerFile.uploadProgress();
    return p > 0 && p < 100;
  });

  onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file && /\.(stl|obj|3mf)$/i.test(file.name)) {
      void this.startWorkplate(file);
    }
    input.value = '';
  }

  private async startWorkplate(file: File): Promise<void> {
    try {
      const workplate = await this.slicer.startWorkplate(file);
      // Carry the upload response in router state so the slice viewer can
      // pick up the `ofids` without an extra fetch. On a cold reload the
      // viewer falls back to `GET /api/request/:ruuid`.
      this.router.navigate(['/slice', workplate.requestUuid], {
        state: workplate.uploadMeta ? { uploadMeta: workplate.uploadMeta } : undefined,
      });
    } catch {
      // Error is tracked in slicerFile.uploadError
    }
  }
}
