import { provideHttpClient } from '@angular/common/http';
import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withViewTransitions } from '@angular/router';
import { provideMarkdown } from 'ngx-markdown';
import { APP_ROUTES } from './app-routes';
import { UploadGuard } from './services/upload-guard';
import { InputModalityService } from './shared/input-modality/input-modality.service';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(APP_ROUTES, withViewTransitions()),
    provideHttpClient(),
    provideMarkdown(),
    UploadGuard,
    // Eagerly instantiate so body modality classes are stamped from first interaction.
    InputModalityService,
  ],
};
