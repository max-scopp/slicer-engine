import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { NotificationCenter } from './components/notification-center/notification-center';
import { DialogOutlet } from './shared/dialog/dialog-outlet';

@Component({
  selector: 'nexus-root',
  standalone: true,
  imports: [RouterOutlet, NotificationCenter, DialogOutlet],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {}
