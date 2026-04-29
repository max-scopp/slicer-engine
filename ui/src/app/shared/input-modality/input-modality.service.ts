import { InputModalityDetector } from '@angular/cdk/a11y';
import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';

export type InputModality = 'mouse' | 'keyboard' | 'touch' | null;

const MODALITY_CLASSES: Record<NonNullable<InputModality>, string> = {
  mouse: 'nexus-mouse-user',
  keyboard: 'nexus-keyboard-user',
  touch: 'nexus-touch-user',
};

/**
 * Tracks how the user is currently interacting with the page.
 *
 * Built on top of the Angular CDK InputModalityDetector which listens to
 * native pointer, keyboard, and touch events at the document level.
 * Components and directives can inject this service instead of each
 * implementing their own modality checks.
 *
 * Also stamps a `nexus-<modality>-user` class on <body> so global CSS
 * (e.g. focus ring suppression) can key off the current modality without
 * any per-element Angular binding.
 */
@Injectable({ providedIn: 'root' })
export class InputModality {
  private readonly detector = inject(InputModalityDetector);
  private readonly document = inject(DOCUMENT);

  /** Signal that updates whenever the active input modality changes. */
  readonly modality = toSignal(this.detector.modalityChanged, {
    initialValue: this.detector.mostRecentModality,
  });

  /** Observable that emits whenever the active input modality changes. */
  readonly modalityChanged$ = this.detector.modalityChanged;

  constructor() {
    this.detector.modalityChanged.subscribe((modality) => {
      const body = this.document.body;

      for (const cls of Object.values(MODALITY_CLASSES)) {
        body.classList.remove(cls);
      }

      if (modality !== null) {
        body.classList.add(MODALITY_CLASSES[modality]);
      }
    });
  }
}
