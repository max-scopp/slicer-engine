import { Component, ElementRef, afterNextRender, effect, inject, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Slicer } from '../../services/slicer';

@Component({
  selector: 'nexus-status-panel',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './status-panel.component.html',
  styleUrl: './status-panel.component.scss',
})
export class StatusPanel {
  private readonly slicer = inject(Slicer);

  private readonly logContainer = viewChild<ElementRef<HTMLElement>>('logContainer');

  readonly status = this.slicer.status;
  readonly outputLog = this.slicer.outputLog;
  readonly selectedFile = this.slicer.selectedFile;
  readonly phaseTimings = this.slicer.phaseTimings;
  autoScroll = true;

  constructor() {
    // Auto-scroll when output log changes
    effect(() => {
      this.outputLog();
      if (this.autoScroll) {
        this.scrollToBottom();
      }
    });

    afterNextRender(() => {
      if (this.autoScroll) {
        this.scrollToBottom();
      }
    });
  }

  private scrollToBottom(): void {
    const el = this.logContainer()?.nativeElement;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }

  slice(): void {
    void this.slicer.slice();
  }

  reset(): void {
    this.slicer.reset();
  }
}
