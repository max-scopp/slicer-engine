import { Component, inject } from '@angular/core';
import globalSettingsSchema from '../../../schemas/slicer-engine-global-settings-v1.json';
import { FieldChangeEvent, SchemaFormComponent } from '../../schema-form/schema-form.component';
import { Slicer } from '../../services/slicer';

// Extract the SlicingParams sub-schema so the form renders all slicer settings.
// (`SlicingParams` is now the wire-format type — the legacy `WsSlicingParams`
// has been collapsed into a Rust type alias for it, so every form field
// reaches the slicer pipeline as-is.)
const SLICING_PARAMS_SCHEMA = {
  ...(globalSettingsSchema.$defs.SlicingParams as Record<string, unknown>),
  $defs: globalSettingsSchema.$defs as Record<string, unknown>,
};

@Component({
  selector: 'nexus-settings-panel',
  standalone: true,
  imports: [SchemaFormComponent],
  templateUrl: './settings-panel.component.html',
  styleUrl: './settings-panel.component.scss',
})
export class SettingsPanelComponent {
  private readonly slicer = inject(Slicer);

  readonly settings = this.slicer.settings;
  readonly schema = SLICING_PARAMS_SCHEMA;

  update(event: FieldChangeEvent): void {
    this.slicer.updateSettings({ [event.key]: event.value });
  }
}
