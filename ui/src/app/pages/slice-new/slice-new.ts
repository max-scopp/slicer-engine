import { Component, computed, inject } from '@angular/core';
import { Router } from '@angular/router';
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
  readonly slicerFile = inject(SlicerFile);
  readonly uploading = computed(() => {
    const p = this.slicerFile.uploadProgress();
    return p > 0 && p < 100;
  });

  onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file && /\.(stl|obj|3mf)$/i.test(file.name)) {
      this.slicerFile.selectFile(file);
      this.uploadAndNavigate();
    }
    input.value = '';
  }

  private async uploadAndNavigate(): Promise<void> {
    try {
      const meta = await this.slicerFile.upload();
      // Carry the upload response in router state so the slice viewer can
      // pick up the `ofids` without an extra fetch. On a cold reload the
      // viewer falls back to `GET /api/request/:ruuid`.
      this.router.navigate(['/slice', meta.ruuid], { state: { uploadMeta: meta } });
    } catch {
      // Error is tracked in slicerFile.uploadError
    }
  }
}
