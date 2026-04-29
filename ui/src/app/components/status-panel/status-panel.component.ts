import { AfterViewInit, Component, effect, inject, ViewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Slicer } from '../../services/slicer';

@Component({
  selector: 'nexus-status-panel',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './status-panel.component.html',
  styleUrl: './status-panel.component.scss',
})
export class StatusPanelComponent implements AfterViewInit {
  private readonly slicer = inject(Slicer);

  @ViewChild('logContainer') logContainer: any;

  readonly status = this.slicer.status;
  readonly outputLog = this.slicer.outputLog;
  readonly selectedFile = this.slicer.selectedFile;
  readonly phaseTimings = this.slicer.phaseTimings;
  autoScroll = true;

  constructor() {
    // Auto-scroll when output log changes
    effect(() => {
      if (this.autoScroll && this.logContainer) {
        // Use setTimeout to ensure DOM is updated
        setTimeout(() => {
          this.scrollToBottom();
        }, 0);
      }
      // Access outputLog to create dependency
      this.outputLog();
    });
  }

  ngAfterViewInit(): void {
    // Initial scroll to bottom
    if (this.autoScroll) {
      this.scrollToBottom();
    }
  }

  private scrollToBottom(): void {
    if (this.logContainer) {
      this.logContainer.nativeElement.scrollTop = this.logContainer.nativeElement.scrollHeight;
    }
  }

  slice(): void {
    void this.slicer.slice();
  }

  reset(): void {
    this.slicer.reset();
  }
}
