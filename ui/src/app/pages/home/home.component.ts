import { Component, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { ListHistoryComponent } from '../../components/list-history/list-history.component';
import { AppTheme } from '../../services/app-theme';

@Component({
  selector: 'nexus-home-dashboard',
  standalone: true,
  imports: [RouterLink, ListHistoryComponent],
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
})
export class HomeDashboardComponent {
  protected _theme = inject(AppTheme);
}
