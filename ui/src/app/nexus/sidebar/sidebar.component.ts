import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { ConnectionState } from '../../components/connection-state/connection-state';
import { Logo } from '../../components/logo/logo';

@Component({
  selector: 'nexus-nexus-sidebar',
  standalone: true,
  imports: [Logo, RouterLink, ConnectionState],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
})
export class Sidebar {}
