import { provideHttpClient } from '@angular/common/http';
import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { provideRouter, withViewTransitions } from '@angular/router';
import { provideMarkdown } from 'ngx-markdown';
import { APP_ROUTES } from './app-routes';
import { KeyboardShortcuts } from './services/keyboard-shortcuts/keyboard-shortcuts';
import { UploadGuard } from './services/upload-guard';
import { UserInputModality } from './shared/input-modality/input-modality';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(APP_ROUTES, withViewTransitions()),
    provideHttpClient(),
    provideMarkdown(),
    provideAppInitializer(() => {
      inject(KeyboardShortcuts);
      inject(UserInputModality);
      inject(UploadGuard);
      inject(UserInputModality);
    }),
  ],
};
