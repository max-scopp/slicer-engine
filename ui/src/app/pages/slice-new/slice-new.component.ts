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
export class SliceNewComponent {
  private readonly router = inject(Router);
  readonly slicerFile = inject(SlicerFile);
  readonly uploading = computed(() => {
    const p = this.slicerFile.uploadProgress();
    return p > 0 && p < 100;
  });

  onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      this.slicerFile.selectFile(file);
      this.uploadAndNavigate();
    }
  }

  private async uploadAndNavigate(): Promise<void> {
    try {
      const uuid = await this.slicerFile.upload();
      this.router.navigate(['/slice', uuid]);
    } catch {
      // Error is tracked in slicerFile.uploadError
    }
  }
}
