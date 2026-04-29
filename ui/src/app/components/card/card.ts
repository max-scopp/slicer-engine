import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'nexus-card',
  imports: [],
  templateUrl: './card.html',
  styleUrl: './card.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '[class.small]': "visualSize() === 'small'",
    '[class.large]': "visualSize() === 'large'",
  },
})
export class Card {
  readonly visualSize = input<'small' | 'large'>('small');
}
