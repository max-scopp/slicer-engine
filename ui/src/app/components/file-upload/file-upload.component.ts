import { Component, inject } from '@angular/core';
import { Slicer } from '../../services/slicer';

const SUPPORTED_EXTENSIONS = ['.stl', '.obj', '.3mf'] as const;

function hasSupportedExtension(file: File): boolean {
  const name = file.name.toLowerCase();
  return SUPPORTED_EXTENSIONS.some((ext) => name.endsWith(ext));
}

@Component({
  selector: 'nexus-file-upload',
  standalone: true,
  templateUrl: './file-upload.component.html',
  styleUrl: './file-upload.component.scss',
})
export class FileUploadComponent {
  private readonly slicer = inject(Slicer);

  readonly selectedFile = this.slicer.selectedFile;
  isDragging = false;

  onDragOver(event: DragEvent): void {
    event.preventDefault();
    this.isDragging = true;
  }

  onDragLeave(): void {
    this.isDragging = false;
  }

  onDrop(event: DragEvent): void {
    event.preventDefault();
    this.isDragging = false;
    const file = event.dataTransfer?.files[0];
    if (file && hasSupportedExtension(file)) {
      this.slicer.selectFile(file);
    }
  }

  onFileInput(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file && hasSupportedExtension(file)) {
      this.slicer.selectFile(file);
    }
    input.value = '';
  }
}
