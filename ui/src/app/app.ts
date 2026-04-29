import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ConnectionState } from './components/connection-state/connection-state';

@Component({
  selector: 'nexus-root',
  standalone: true,
  imports: [RouterOutlet, ConnectionState],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {}
