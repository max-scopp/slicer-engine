import { Routes } from '@angular/router';
import { NexusSlicingShell } from './nexus/layout/slicing-shell/slicing-shell';
import { uploadCanDeactivate } from './services/upload-guard';

export const APP_ROUTES: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./pages/home/home.component').then((m) => m.HomeDashboardComponent),
  },
  {
    path: 'slice',
    component: NexusSlicingShell,
    children: [
      { path: '', redirectTo: 'new', pathMatch: 'full' },
      {
        path: 'new',
        loadComponent: () =>
          import('./pages/slice-new/slice-new.component').then((m) => m.SliceNewComponent),
        canDeactivate: [uploadCanDeactivate],
      },
      {
        path: ':requestUuid',
        loadComponent: () =>
          import('./pages/slice-viewer/slice-viewer.component').then((m) => m.SliceViewerComponent),
      },
    ],
  },
];
