import { DestroyRef, inject, Injectable } from '@angular/core';
import { takeUntilDestroyed, toObservable } from '@angular/core/rxjs-interop';
import { CanDeactivateFn } from '@angular/router';
import { fromEvent } from 'rxjs';
import { switchMap } from 'rxjs/operators';
import { SlicerFile } from './slicer-file';

/**
 * Registers a `beforeunload` listener whenever an upload is in progress,
 * prompting the browser to confirm before the tab is closed or refreshed.
 * Also exposes a `canDeactivate` guard for the router.
 */
@Injectable({ providedIn: 'root' })
export class UploadGuard {
  readonly #slicerFile = inject(SlicerFile);
  readonly #destroyRef = inject(DestroyRef);

  constructor() {
    // Convert the uploadProgress signal to an observable, then switch to a
    // fromEvent subscription only while an upload is active.
    toObservable(this.#slicerFile.isUploading)
      .pipe(
        switchMap((uploading) => {
          return uploading ? fromEvent<BeforeUnloadEvent>(window, 'beforeunload') : [];
        }),
        takeUntilDestroyed(this.#destroyRef),
      )
      .subscribe((event) => {
        event.preventDefault();
      });
  }

  canLeave(): boolean {
    if (!this.#slicerFile.isUploading()) {
      return true;
    }

    return window.confirm('An upload is in progress. Are you sure you want to leave?');
  }
}

export const uploadCanDeactivate: CanDeactivateFn<unknown> = () => {
  return inject(UploadGuard).canLeave();
};
