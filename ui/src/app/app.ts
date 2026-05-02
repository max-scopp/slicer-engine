import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { NotificationCenter } from './components/notification-center/notification-center';

@Component({
  selector: 'nexus-root',
  standalone: true,
  imports: [RouterOutlet, NotificationCenter],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {}
