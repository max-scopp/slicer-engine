import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'nexus-tooltip',
  templateUrl: './tooltip.component.html',
  styleUrl: './tooltip.component.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TooltipComponent {
  readonly text = input.required<string>();
}
