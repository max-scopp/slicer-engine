import { ChangeDetectionStrategy, Component, computed, effect, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';
import { map } from 'rxjs';
import { Viewer, ViewerMode } from '../../components/viewer';
import { NotificationService } from '../../services/notifications';
import { Slicer } from '../../services/slicer';
import { SlicerFile } from '../../services/slicer-file';

@Component({
  selector: 'nexus-slice-viewer',
  standalone: true,
  imports: [Viewer],
  templateUrl: './slice-viewer.component.html',
  styleUrl: './slice-viewer.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SliceViewerComponent {
  readonly #activatedRoute = inject(ActivatedRoute);
  readonly #slicer = inject(Slicer);
  readonly #slicerFile = inject(SlicerFile);
  readonly #notifications = inject(NotificationService);

  readonly requestUuid = toSignal(
    this.#activatedRoute.params.pipe(map((params) => params['requestUuid'] as string | undefined)),
  );

  /** The user-selected STL (when available) is shown in model mode. */
  readonly modelFile = this.#slicerFile.selectedFile;

  /**
   * Default to the raw-mesh viewer until a slice is finished. We only switch to
   * the G-code preview once the slicer reports `done` for the current session.
   */
  readonly viewerMode = computed<ViewerMode>(() => {
    const status = this.#slicer.status();
    return status === 'done' ? 'gcode' : 'model';
  });

  #lastFetchedUuid: string | null = null;

  constructor() {
    // Always reload the STL whenever the route UUID changes — the in-memory
    // file may belong to a different request (e.g. navigating between history
    // entries) or may be absent entirely (reload / deep-link).
    // Guard against double-fire (toSignal init + first emission for same UUID).
    effect(() => {
      const uuid = this.requestUuid();
      if (!uuid || uuid === this.#lastFetchedUuid) {
        return;
      }

      this.#lastFetchedUuid = uuid;
      void this.#restoreModelFromBackend(uuid);
    });
  }

  async #restoreModelFromBackend(requestUuid: string): Promise<void> {
    let notifId: string | null = null;

    try {
      const meta = await this.#slicerFile.getRequestMeta(requestUuid);

      if (!meta.has_stl) {
        return;
      }

      const filename = meta.original_filename ?? 'model.stl';
      notifId = this.#notifications.progress('Loading model…', `Fetching ${filename} from server`);

      await this.#slicerFile.fetchStlForRequest(requestUuid, filename);

      this.#notifications.completeProgress(notifId, 'Model loaded', filename);
    } catch {
      if (notifId) {
        this.#notifications.failProgress(
          notifId,
          'Failed to load model',
          'The STL file could not be retrieved from the server.',
        );
      }
    }
  }
}
