import { Component, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { AppTheme } from '../../services/app-theme';

@Component({
  selector: 'nexus-home-dashboard',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
})
export class HomeDashboardComponent {
  protected _theme = inject(AppTheme);
}
