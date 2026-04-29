import { Component, ElementRef, ViewChild, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { ConnectionState } from '../../components/connection-state/connection-state';
import { ListHistoryComponent } from '../../components/list-history/list-history.component';
import { AppTheme } from '../../services/app-theme';
import { SlicerFile } from '../../services/slicer-file';

@Component({
  selector: 'nexus-home-dashboard',
  standalone: true,
  imports: [RouterLink, ListHistoryComponent, ConnectionState],
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
})
export class HomeDashboardComponent {
  protected _theme = inject(AppTheme);
  private readonly router = inject(Router);
  private readonly slicerFile = inject(SlicerFile);

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
    this.slicerFile.selectFile(file);
    try {
      const uuid = await this.slicerFile.upload();
      this.router.navigate(['/slice', uuid]);
    } catch {
      // upload error is tracked in slicerFile.uploadError
    }
  }
}
