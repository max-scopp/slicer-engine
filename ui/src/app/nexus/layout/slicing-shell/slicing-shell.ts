import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ThreeDViewToolbar } from '../../../components/3d-view-toolbar/3d-view-toolbar';
import { SettingsPanelComponent } from '../../../components/settings-panel/settings-panel.component';
import { ViewportCube } from '../../../components/viewport-cube/viewport-cube';
import { PrintEstimates } from '../../print-estimates/print-estimates';
import { Sidebar } from '../../sidebar/sidebar.component';

@Component({
  selector: 'nexus-slicing-shell',
  imports: [
    Sidebar,
    PrintEstimates,
    ThreeDViewToolbar,
    ViewportCube,
    RouterOutlet,
    SettingsPanelComponent,
  ],
  templateUrl: './slicing-shell.html',
  styleUrl: './slicing-shell.scss',
})
export class NexusSlicingShell {}
