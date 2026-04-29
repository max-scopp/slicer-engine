import { HttpClient, HttpEventType } from '@angular/common/http';
import { Injectable, computed, inject, signal } from '@angular/core';
import { environment } from '../../environments/environment';

export interface RequestMeta {
  request_uuid: string;
  status: string;
  original_filename: string | null;
  has_stl: boolean;
  has_gcode: boolean;
}

@Injectable({ providedIn: 'root' })
export class SlicerFile {
  readonly #http = inject(HttpClient);

  readonly selectedFile = signal<File | null>(null);
  readonly requestUuid = signal<string | null>(null);
  readonly uploadProgress = signal<number>(0);
  readonly uploadError = signal<string | null>(null);
  readonly isUploading = computed(() => this.uploadProgress() > 0 && this.uploadProgress() < 100);
  readonly isPending = computed(
    () =>
      this.selectedFile() !== null && this.uploadProgress() === 0 && this.uploadError() === null,
  );

  selectFile(file: File): void {
    this.selectedFile.set(file);
    this.uploadProgress.set(0);
    this.uploadError.set(null);
  }

  upload(): Promise<string> {
    const file = this.selectedFile();
    if (!file) {
      throw new Error('No file selected');
    }

    this.uploadProgress.set(0);
    this.uploadError.set(null);

    const formData = new FormData();
    formData.append('file', file);

    return new Promise((resolve, reject) => {
      this.#http
        .post<{ request_uuid: string }>(`${environment.apiUrl}/upload`, formData, {
          reportProgress: true,
          observe: 'events',
        })
        .subscribe({
          next: (event) => {
            if (event.type === HttpEventType.UploadProgress) {
              const progress = event.total ? Math.round((event.loaded / event.total) * 100) : 0;
              this.uploadProgress.set(progress);
            } else if (event.type === HttpEventType.Response) {
              const uuid = event.body!.request_uuid;
              this.requestUuid.set(uuid);
              this.uploadProgress.set(100);
              resolve(uuid);
            }
          },
          error: (error) => {
            const message = error instanceof Error ? error.message : 'Upload failed';
            this.uploadError.set(message);
            reject(error);
          },
        });
    });
  }

  reset(): void {
    this.selectedFile.set(null);
    this.requestUuid.set(null);
    this.uploadProgress.set(0);
    this.uploadError.set(null);
  }

  /** Fetch request metadata for a given UUID. */
  getRequestMeta(requestUuid: string): Promise<RequestMeta> {
    return this.#http
      .get<RequestMeta>(`${environment.apiUrl}/request/${requestUuid}`)
      .toPromise()
      .then((meta) => {
        if (!meta) {
          throw new Error('No response from server');
        }
        return meta;
      });
  }

  /**
   * Fetch the STL file from the backend for an existing request and restore
   * it as the currently-selected file so the scene can display it.
   * Reports download progress via `uploadProgress`.
   */
  fetchStlForRequest(requestUuid: string, filename: string): Promise<void> {
    this.uploadProgress.set(0);
    this.uploadError.set(null);

    return new Promise((resolve, reject) => {
      this.#http
        .get(`${environment.apiUrl}/stl/${requestUuid}`, {
          responseType: 'blob',
          reportProgress: true,
          observe: 'events',
        })
        .subscribe({
          next: (event) => {
            if (event.type === HttpEventType.DownloadProgress) {
              const progress = event.total ? Math.round((event.loaded / event.total) * 100) : 0;
              this.uploadProgress.set(progress);
            } else if (event.type === HttpEventType.Response) {
              const blob = event.body as Blob;
              const file = new File([blob], filename, {
                type: 'application/octet-stream',
              });
              this.selectedFile.set(file);
              this.requestUuid.set(requestUuid);
              this.uploadProgress.set(100);
              resolve();
            }
          },
          error: (error: unknown) => {
            const message = error instanceof Error ? error.message : 'Failed to load model';
            this.uploadError.set(message);
            reject(error);
          },
        });
    });
  }
}
