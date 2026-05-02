import { Component, ElementRef, ViewChild, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { ConnectionState } from '../../components/connection-state/connection-state';
import { ListHistory } from '../../components/list-history/list-history';
import { AppTheme } from '../../services/app-theme';
import { Slicer } from '../../services/slicer';

@Component({
  selector: 'nexus-home-dashboard',
  standalone: true,
  imports: [RouterLink, ListHistory, ConnectionState],
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
})
export class HomeDashboard {
  protected _theme = inject(AppTheme);
  private readonly router = inject(Router);
  private readonly slicer = inject(Slicer);

  @ViewChild('quickFileInput') private quickFileInput!: ElementRef<HTMLInputElement>;

  openQuickSlice(): void {
    this.quickFileInput.nativeElement.click();
  }

  async onQuickFileSelected(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !/\.(stl|obj|3mf)$/i.test(file.name)) {
      return;
    }
    try {
      const workplate = await this.slicer.startWorkplate(file);
      this.router.navigate(['/slice', workplate.requestUuid], {
        state: workplate.uploadMeta ? { uploadMeta: workplate.uploadMeta } : undefined,
      });
    } catch {
      // Errors are tracked by the slicer/file services and surfaced in the UI.
    }
  }
}
