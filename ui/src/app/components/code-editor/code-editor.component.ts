import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  inject,
  viewChild,
} from '@angular/core';
import type * as Monaco from 'monaco-editor';

// Extend the window type to allow the MonacoEnvironment global required by the
// Monaco editor loader.
declare global {
  interface Window {
    MonacoEnvironment?: Monaco.Environment;
  }
}

/**
 * Thin Angular wrapper around the Monaco editor.
 *
 * The component initialises the editor exactly once, after the host element
 * has been inserted into the DOM. Destroying the component disposes the
 * editor instance so its WebGL / DOM resources are released.
 *
 * The editor is intentionally bare-bones: language, initial value and other
 * options can be extended via `@Input()` when needed.
 */
@Component({
  selector: 'nexus-code-editor',
  standalone: true,
  template: `<div class="editor-mount" #mount></div>`,
  styles: [
    `
      :host {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        overflow: hidden;
        background: var(--color-surface, #1e1e1e);
      }

      .editor-mount {
        flex: 1;
        min-height: 0;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CodeEditorComponent {
  private readonly mount = viewChild.required<ElementRef<HTMLDivElement>>('mount');
  private editor: Monaco.editor.IStandaloneCodeEditor | null = null;

  constructor() {
    const destroyRef = inject(DestroyRef);

    // afterNextRender fires once after the host is in the DOM — safe to
    // call the imperative Monaco API here.
    afterNextRender(async () => {
      await this.initMonaco();
    });

    destroyRef.onDestroy(() => {
      this.editor?.dispose();
      this.editor = null;
    });
  }

  private async initMonaco(): Promise<void> {
    // Tell Monaco where to find its web workers. Blob URLs allow workers
    // to be spawned without a separate worker bundle entry point.
    if (!window.MonacoEnvironment) {
      window.MonacoEnvironment = {
        getWorker(_moduleId: string, label: string): Worker {
          const workerUrls: Record<string, string> = {
            json: 'monaco-editor/esm/vs/language/json/json.worker',
            css: 'monaco-editor/esm/vs/language/css/css.worker',
            html: 'monaco-editor/esm/vs/language/html/html.worker',
            typescript: 'monaco-editor/esm/vs/language/typescript/ts.worker',
            javascript: 'monaco-editor/esm/vs/language/typescript/ts.worker',
          };
          const workerPath = workerUrls[label] ?? 'monaco-editor/esm/vs/editor/editor.worker';
          const blob = new Blob([`importScripts('${workerPath}');`], {
            type: 'application/javascript',
          });
          return new Worker(URL.createObjectURL(blob));
        },
      };
    }

    // Dynamic import keeps the large Monaco bundle out of the initial
    // chunk — it is only fetched when the panel is first opened.
    const monaco = await import('monaco-editor');

    this.editor = monaco.editor.create(this.mount().nativeElement, {
      value: '',
      language: 'plaintext',
      theme: 'vs-dark',
      automaticLayout: true,
      fontSize: 13,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      wordWrap: 'on',
      lineNumbers: 'on',
    });
  }
}
