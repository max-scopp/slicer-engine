import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ThreeDViewToolbar } from '../../../components/3d-view-toolbar/3d-view-toolbar';
import { PrintEstimates } from '../../print-estimates/print-estimates';
import { Sidebar } from '../../sidebar/sidebar.component';

@Component({
  selector: 'nexus-slicing-shell',
  imports: [Sidebar, PrintEstimates, ThreeDViewToolbar, RouterOutlet],
  templateUrl: './slicing-shell.html',
  styleUrl: './slicing-shell.scss',
})
export class NexusSlicingShell {}
