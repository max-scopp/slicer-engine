import { Component, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { SlicerService } from '../../services/slicer.service';
import { SliceSettings } from '../../models/slice-settings.model';

@Component({
  selector: 'app-settings-panel',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './settings-panel.component.html',
  styleUrl: './settings-panel.component.scss',
})
export class SettingsPanelComponent {
  private readonly slicer = inject(SlicerService);

  readonly settings = this.slicer.settings;

  update(patch: Partial<SliceSettings>): void {
    this.slicer.updateSettings(patch);
  }
}
