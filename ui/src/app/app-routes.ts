import { Routes } from '@angular/router';
import { NexusSlicingShell } from './nexus/layout/slicing-shell/slicing-shell';
import { uploadCanDeactivate } from './services/upload-guard';

export const APP_ROUTES: Routes = [
  {
    path: '',
    loadComponent: async () => import('./pages/home/home').then((m) => m.HomeDashboard),
  },
  {
    path: 'slice',
    component: NexusSlicingShell,
    children: [
      { path: '', redirectTo: 'new', pathMatch: 'full' },
      {
        path: 'new',
        loadComponent: () => import('./pages/slice-new/slice-new').then((m) => m.SliceNew),
        canDeactivate: [uploadCanDeactivate],
      },
      {
        path: ':requestUuid',
        loadComponent: () => import('./pages/slice-viewer/slice-viewer').then((m) => m.SliceViewer),
      },
    ],
  },
];
