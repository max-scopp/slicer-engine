import { Component, inject } from '@angular/core';
import { SlicerService } from '../../services/slicer.service';

@Component({
  selector: 'app-status-panel',
  standalone: true,
  templateUrl: './status-panel.component.html',
  styleUrl: './status-panel.component.scss',
})
export class StatusPanelComponent {
  private readonly slicer = inject(SlicerService);

  readonly status = this.slicer.status;
  readonly outputLog = this.slicer.outputLog;
  readonly selectedFile = this.slicer.selectedFile;

  slice(): void {
    this.slicer.slice();
  }

  reset(): void {
    this.slicer.reset();
  }
}
