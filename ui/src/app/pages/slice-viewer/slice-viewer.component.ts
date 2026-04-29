import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';
import { map } from 'rxjs';
import { environment } from '../../../environments/environment';
import { ViewerComponent, ViewerMode } from '../../components/viewer';
import { Slicer } from '../../services/slicer';
import { SlicerFile } from '../../services/slicer-file';

@Component({
  selector: 'nexus-slice-viewer',
  standalone: true,
  imports: [ViewerComponent],
  templateUrl: './slice-viewer.component.html',
  styleUrl: './slice-viewer.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SliceViewerComponent {
  readonly #activatedRoute = inject(ActivatedRoute);
  readonly #slicer = inject(Slicer);
  readonly #slicerFile = inject(SlicerFile);

  readonly requestUuid = toSignal(
    this.#activatedRoute.params.pipe(map((params) => params['requestUuid'] as string | undefined)),
  );

  /** The user-selected STL (when available) is shown in model mode. */
  readonly modelFile = this.#slicerFile.selectedFile;

  /** G-code download URL for the current request, used only in gcode mode. */
  readonly gcodeUrl = computed<string | null>(() => {
    const uuid = this.requestUuid();
    if (!uuid) {
      return null;
    }
    return `${environment.apiUrl}/download/${uuid}`;
  });

  /**
   * Default to the raw-mesh viewer until a slice is finished. We only switch to
   * the G-code preview once the slicer reports `done` for the current session.
   */
  readonly viewerMode = computed<ViewerMode>(() => {
    const status = this.#slicer.status();
    return status === 'done' ? 'gcode' : 'model';
  });
}
