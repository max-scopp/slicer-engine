import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable, shareReplay } from 'rxjs';

@Injectable({ providedIn: 'root' })
export class IconCache {
  private readonly http = inject(HttpClient);
  private readonly cache = new Map<string, Observable<string>>();

  get(url: string): Observable<string> {
    let cached = this.cache.get(url);
    if (!cached) {
      cached = this.http.get(url, { responseType: 'text' }).pipe(shareReplay(1));
      this.cache.set(url, cached);
    }
    return cached;
  }
}
