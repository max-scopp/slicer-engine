import { HttpClient, HttpEventType } from '@angular/common/http';
import { Injectable, computed, inject, signal } from '@angular/core';
import { environment } from '../../environments/environment';

/**
 * Metadata for a workplate, returned by `GET /api/request/:request_uuid`.
 *
 * `ofids` is the list of files (by `file_uuid`) that were placed in this
 * workplate. The slicer references each file by its own UUID — distinct
 * from the workplate `request_uuid`/`ruuid`.
 */
export interface RequestMeta {
  ruuid: string;
  status: string;
  has_gcode: boolean;
  ofids: { file_uuid: string; original_filename: string }[];
}

/**
 * Response from `POST /api/upload` — the workplate UUID plus the list of
 * file UUIDs that were created. Today there is exactly one file per upload,
 * but the protocol is multi-file ready.
 */
export interface UploadResponse {
  ruuid: string;
  ofids: string[];
}

@Injectable({ providedIn: 'root' })
export class SlicerFile {
  readonly #http = inject(HttpClient);

  readonly selectedFile = signal<File | null>(null);
  /** Workplate UUID — the `ruuid` from the upload response. */
  readonly requestUuid = signal<string | null>(null);
  /** File UUIDs (`ofids`) that belong to {@link requestUuid}. */
  readonly fileIds = signal<string[]>([]);
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

  upload(): Promise<UploadResponse> {
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
        .post<UploadResponse>(`${environment.apiUrl}/upload`, formData, {
          reportProgress: true,
          observe: 'events',
        })
        .subscribe({
          next: (event) => {
            if (event.type === HttpEventType.UploadProgress) {
              const progress = event.total ? Math.round((event.loaded / event.total) * 100) : 0;
              this.uploadProgress.set(progress);
            } else if (event.type === HttpEventType.Response) {
              const body = event.body!;
              this.requestUuid.set(body.ruuid);
              this.fileIds.set(body.ofids);
              this.uploadProgress.set(100);
              resolve(body);
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
    this.fileIds.set([]);
    this.uploadProgress.set(0);
    this.uploadError.set(null);
  }

  /** Fetch workplate metadata for a given `ruuid`. */
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
   * Adopt the result of a previous upload (e.g. carried in route data) so the
   * slice flow can pick up where the user left off without re-fetching.
   */
  adopt(meta: RequestMeta): void {
    this.requestUuid.set(meta.ruuid);
    this.fileIds.set(meta.ofids.map((f) => f.file_uuid));
  }

  /**
   * Fetch an uploaded file from the backend by its `file_uuid` and restore
   * it as the currently-selected file so the scene can display it.
   * Reports download progress via `uploadProgress`.
   */
  fetchFile(requestUuid: string, fileUuid: string, filename: string): Promise<void> {
    this.uploadProgress.set(0);
    this.uploadError.set(null);

    return new Promise((resolve, reject) => {
      this.#http
        .get(`${environment.apiUrl}/file/${fileUuid}`, {
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
              this.fileIds.set([fileUuid]);
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
