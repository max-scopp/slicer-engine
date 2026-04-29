import { Component, input } from '@angular/core';

export type BadgeVariant = 'default' | 'success' | 'warning' | 'danger' | 'info';

@Component({
  selector: 'nexus-badge',
  standalone: true,
  templateUrl: './badge.html',
  styleUrl: './badge.scss',
  host: {
    '[attr.variant]': 'variant()',
  },
})
export class Badge {
  readonly variant = input<BadgeVariant>('default');
}
